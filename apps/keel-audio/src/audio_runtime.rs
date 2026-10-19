//! Real-time capture / monitor loop.
//!
//! Opens the Wave (or preferred) Core Audio input, feeds
//! [`keel_audio_engine::AudioEngine::process_block`], mixes to Personal,
//! plays to the main output, and pushes frames into the HAL shm client.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use keel_audio_engine::AudioEngine;
use keel_audio_hal::HalClient;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

pub struct AudioRuntime {
    _input: Option<cpal::Stream>,
    _output: Option<cpal::Stream>,
    pub monitor_ring: Arc<Mutex<Vec<f32>>>,
}

impl AudioRuntime {
    pub fn start(
        engine: Arc<AudioEngine>,
        hal: Arc<Mutex<HalClient>>,
        preferred_input_name: Option<String>,
    ) -> Result<Self, String> {
        let host = cpal::default_host();
        let input = pick_input(&host, preferred_input_name.as_deref())?;
        let output = pick_output(&host, &engine)?;

        let in_name = input.name().unwrap_or_else(|_| "input".into());
        let out_name = output.name().unwrap_or_else(|_| "output".into());
        info!("audio runtime: capture '{in_name}' → monitor '{out_name}'");

        let in_cfg = input
            .default_input_config()
            .map_err(|e| format!("input config: {e}"))?;
        let out_cfg = output
            .default_output_config()
            .map_err(|e| format!("output config: {e}"))?;

        let in_channels = in_cfg.channels();
        let stream_cfg: StreamConfig = in_cfg.clone().into();
        let channel_id = resolve_wave_channel_id(&engine, &in_name);
        ensure_default_routing(&engine, channel_id);

        let monitor_ring = Arc::new(Mutex::new(Vec::new()));
        let ring_in = Arc::clone(&monitor_ring);
        let ring_out = Arc::clone(&monitor_ring);
        let engine_in = Arc::clone(&engine);
        let hal_in = Arc::clone(&hal);

        let err_fn = |e| warn!("audio stream error: {e}");

        let input_stream = match in_cfg.sample_format() {
            SampleFormat::F32 => input
                .build_input_stream(
                    &stream_cfg,
                    {
                        let engine = Arc::clone(&engine_in);
                        let ring = Arc::clone(&ring_in);
                        let hal = Arc::clone(&hal_in);
                        move |data: &[f32], _| {
                            process_input(data, in_channels, &engine, channel_id, &ring, &hal);
                        }
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("build input: {e}"))?,
            SampleFormat::I16 => input
                .build_input_stream(
                    &stream_cfg,
                    {
                        let engine = Arc::clone(&engine_in);
                        let ring = Arc::clone(&ring_in);
                        let hal = Arc::clone(&hal_in);
                        move |data: &[i16], _| {
                            let f: Vec<f32> =
                                data.iter().map(|s| *s as f32 / i16::MAX as f32).collect();
                            process_input(&f, in_channels, &engine, channel_id, &ring, &hal);
                        }
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("build input: {e}"))?,
            SampleFormat::U16 => input
                .build_input_stream(
                    &stream_cfg,
                    {
                        let engine = Arc::clone(&engine_in);
                        let ring = Arc::clone(&ring_in);
                        let hal = Arc::clone(&hal_in);
                        move |data: &[u16], _| {
                            let f: Vec<f32> = data
                                .iter()
                                .map(|s| (*s as f32 / u16::MAX as f32) * 2.0 - 1.0)
                                .collect();
                            process_input(&f, in_channels, &engine, channel_id, &ring, &hal);
                        }
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("build input: {e}"))?,
            other => return Err(format!("unsupported input format: {other:?}")),
        };

        let out_stream_cfg: StreamConfig = out_cfg.clone().into();
        let out_channels = out_cfg.channels();
        let output_stream = match out_cfg.sample_format() {
            SampleFormat::F32 => output
                .build_output_stream(
                    &out_stream_cfg,
                    {
                        let ring = Arc::clone(&ring_out);
                        move |data: &mut [f32], _| fill_output(data, &ring, out_channels)
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("build output: {e}"))?,
            SampleFormat::I16 => output
                .build_output_stream(
                    &out_stream_cfg,
                    {
                        let ring = Arc::clone(&ring_out);
                        move |data: &mut [i16], _| {
                            let mut tmp = vec![0f32; data.len()];
                            fill_output(&mut tmp, &ring, out_channels);
                            for (d, s) in data.iter_mut().zip(tmp.iter()) {
                                *d = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("build output: {e}"))?,
            other => return Err(format!("unsupported output format: {other:?}")),
        };

        input_stream
            .play()
            .map_err(|e| format!("input play: {e}"))?;
        output_stream
            .play()
            .map_err(|e| format!("output play: {e}"))?;

        Ok(Self {
            _input: Some(input_stream),
            _output: Some(output_stream),
            monitor_ring,
        })
    }
}

fn process_input(
    data: &[f32],
    in_channels: u16,
    engine: &AudioEngine,
    channel_id: Option<Uuid>,
    ring: &Mutex<Vec<f32>>,
    hal: &Mutex<HalClient>,
) {
    let Some(ch) = channel_id else {
        return;
    };
    let stereo = to_stereo_interleaved(data, in_channels);
    let mut mix_bufs: HashMap<Uuid, Vec<f32>> = HashMap::new();
    engine.process_block(ch, &stereo, &mut mix_bufs, false);
    engine.finalize_mixes(&mut mix_bufs);

    let state = engine.state();
    let personal = state
        .mixes
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case("Personal"))
        .map(|m| m.id);
    let buf = personal
        .and_then(|id| mix_bufs.get(&id))
        .or_else(|| mix_bufs.values().next());

    if let Some(b) = buf {
        *ring.lock() = b.clone();
        if let Some(h) = hal.try_lock() {
            let _ = h.write_output(1, b);
        }
    }
}

fn fill_output(data: &mut [f32], ring: &Mutex<Vec<f32>>, channels: u16) {
    let src = ring.lock();
    if src.is_empty() {
        data.fill(0.0);
        return;
    }
    let ch = channels.max(1) as usize;
    if ch >= 2 {
        for (i, frame) in data.chunks_exact_mut(ch).enumerate() {
            let si = (i * 2) % (src.len().max(2) / 2 * 2);
            frame[0] = *src.get(si).unwrap_or(&0.0);
            frame[1] = *src.get(si + 1).unwrap_or(&frame[0]);
            for s in frame.iter_mut().skip(2) {
                *s = 0.0;
            }
        }
    } else {
        for (i, s) in data.iter_mut().enumerate() {
            let si = (i * 2) % src.len().max(1);
            *s = src[si];
        }
    }
}

fn to_stereo_interleaved(data: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        let mut out = Vec::with_capacity(data.len() * 2);
        for &s in data {
            out.push(s);
            out.push(s);
        }
        out
    } else if channels == 2 {
        data.to_vec()
    } else {
        // Downmix first two channels.
        let mut out = Vec::with_capacity(data.len() / channels as usize * 2);
        for frame in data.chunks(channels as usize) {
            out.push(*frame.first().unwrap_or(&0.0));
            out.push(*frame.get(1).unwrap_or(frame.first().unwrap_or(&0.0)));
        }
        out
    }
}

fn resolve_wave_channel_id(engine: &AudioEngine, input_name: &str) -> Option<Uuid> {
    let state = engine.state();
    state
        .channels
        .iter()
        .find(|c| c.is_wave && c.name.eq_ignore_ascii_case(input_name))
        .or_else(|| state.channels.iter().find(|c| c.is_wave))
        .map(|c| c.id)
}

fn ensure_default_routing(engine: &AudioEngine, channel_id: Option<Uuid>) {
    let Some(ch) = channel_id else {
        return;
    };
    let state = engine.state();
    if let Some(mix) = state
        .mixes
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case("Personal"))
    {
        if !mix.routing.contains_key(&ch) {
            engine.set_routing(mix.id, ch, Some(1.0));
            let _ = engine.save();
        }
    }
}

fn pick_input(host: &cpal::Host, preferred: Option<&str>) -> Result<cpal::Device, String> {
    let devices = host
        .input_devices()
        .map_err(|e| format!("list inputs: {e}"))?;
    let mut fallback = None;
    for d in devices {
        let Ok(name) = d.name() else { continue };
        let lc = name.to_lowercase();
        if let Some(p) = preferred {
            if name.eq_ignore_ascii_case(p) {
                return Ok(d);
            }
        }
        if lc.contains("wave") {
            return Ok(d);
        }
        if fallback.is_none() && !lc.contains("iphone") {
            fallback = Some(d);
        }
    }
    fallback.ok_or_else(|| "no input device".into())
}

fn pick_output(host: &cpal::Host, engine: &AudioEngine) -> Result<cpal::Device, String> {
    let state = engine.state();
    let prefer = state.main_output_device_id.clone();
    let mut fallback = host
        .default_output_device()
        .ok_or_else(|| "no default output".to_string())?;
    let devices = host
        .output_devices()
        .map_err(|e| format!("list outputs: {e}"))?;
    for d in devices {
        let Ok(name) = d.name() else { continue };
        if let Some(ref p) = prefer {
            if p != "system_default" && (name.contains(p) || p.contains(&name)) {
                return Ok(d);
            }
        }
        let lc = name.to_lowercase();
        if lc.contains("macbook") || lc.contains("speaker") {
            fallback = d;
        }
    }
    Ok(fallback)
}
