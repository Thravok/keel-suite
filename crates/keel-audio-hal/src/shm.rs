//! POSIX shared-memory ring layout shared by the HAL plug-in and userspace.
//!
//! Layout (bytes):
//! ```text
//! HalShmHeader          (64 bytes, padded)
//! DeviceSlot[MAX_DEVICES]
//! f32 ring planes: MAX_DEVICES * CHANNELS * RING_FRAMES
//! ```

#[cfg(unix)]
use crate::SHM_NAME;
use crate::{CHANNELS, MAGIC, MAX_DEVICES, RING_FRAMES};
use std::io;
use std::sync::atomic::{AtomicU32, Ordering};

#[repr(C)]
pub struct HalShmHeader {
    pub magic: AtomicU32,
    pub version: AtomicU32,
    pub sample_rate: AtomicU32,
    pub device_count: AtomicU32,
    pub write_idx: AtomicU32,
    pub read_idx: AtomicU32,
    pub frame_capacity: AtomicU32,
    pub channels: AtomicU32,
    pub _pad: [u32; 8],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeviceSlot {
    pub id: u32,
    pub flags: u32,
    pub write_idx: u32,
    pub read_idx: u32,
    pub name: [u8; 64],
}

pub const HEADER_SIZE: usize = 64; // keep stable across Rust/C
pub const SLOT_SIZE: usize = 4 + 4 + 4 + 4 + 64; // 80
pub const SLOTS_BYTES: usize = SLOT_SIZE * MAX_DEVICES;
pub const SAMPLES_PER_DEVICE: usize = CHANNELS * RING_FRAMES;
pub const RING_BYTES_PER_DEVICE: usize = SAMPLES_PER_DEVICE * 4;

pub fn default_shm_size() -> usize {
    HEADER_SIZE + SLOTS_BYTES + MAX_DEVICES * RING_BYTES_PER_DEVICE
}

/// Userspace mapping of the HAL ring.
pub struct ShmRing {
    ptr: *mut u8,
    /// Mapping length (used by Drop on unix; kept for layout parity on other targets).
    #[allow(dead_code)]
    len: usize,
    #[cfg(unix)]
    fd: i32,
}

unsafe impl Send for ShmRing {}
unsafe impl Sync for ShmRing {}

impl ShmRing {
    #[cfg(unix)]
    pub fn open_or_create(device_count: u32, sample_rate: u32) -> io::Result<Self> {
        use std::ffi::CString;
        use std::ptr;

        let name = CString::new(format!("/{SHM_NAME}"))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "bad shm name"))?;
        let size = default_shm_size();
        let fd = unsafe { libc::shm_open(name.as_ptr(), libc::O_RDWR | libc::O_CREAT, 0o666) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::ftruncate(fd, size as libc::off_t) } != 0 {
            let err = io::Error::last_os_error();
            unsafe {
                libc::close(fd);
            }
            return Err(err);
        }
        let mapped = unsafe {
            libc::mmap(
                ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if mapped == libc::MAP_FAILED {
            let err = io::Error::last_os_error();
            unsafe {
                libc::close(fd);
            }
            return Err(err);
        }
        let ring = Self {
            ptr: mapped as *mut u8,
            len: size,
            fd,
        };
        ring.ensure_header(device_count, sample_rate);
        Ok(ring)
    }

    #[cfg(not(unix))]
    pub fn open_or_create(_device_count: u32, _sample_rate: u32) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "HAL shm only on unix",
        ))
    }

    #[cfg(unix)]
    fn ensure_header(&self, device_count: u32, sample_rate: u32) {
        let hdr = self.header();
        if hdr.magic.load(Ordering::Acquire) != MAGIC {
            hdr.magic.store(MAGIC, Ordering::Release);
            hdr.version.store(1, Ordering::Release);
            hdr.sample_rate.store(sample_rate, Ordering::Release);
            hdr.device_count
                .store(device_count.min(MAX_DEVICES as u32), Ordering::Release);
            hdr.write_idx.store(0, Ordering::Release);
            hdr.read_idx.store(0, Ordering::Release);
            hdr.frame_capacity
                .store(RING_FRAMES as u32, Ordering::Release);
            hdr.channels.store(CHANNELS as u32, Ordering::Release);
        }
    }

    pub fn header(&self) -> &HalShmHeader {
        unsafe { &*(self.ptr as *const HalShmHeader) }
    }

    pub fn init_device_slot(&self, index: usize, id: u32, name: &str, is_in: bool, is_out: bool) {
        let Some(slot) = self.device_slot_mut(index) else {
            return;
        };
        slot.id = id;
        slot.flags = u32::from(is_in) | (u32::from(is_out) << 1) | (1 << 2);
        slot.write_idx = 0;
        slot.read_idx = 0;
        slot.name = [0; 64];
        let bytes = name.as_bytes();
        let n = bytes.len().min(63);
        slot.name[..n].copy_from_slice(&bytes[..n]);
    }

    // Shared-memory mapping: slots are mutated through an immutable HalShm handle.
    #[allow(clippy::mut_from_ref)]
    fn device_slot_mut(&self, index: usize) -> Option<&mut DeviceSlot> {
        if index >= MAX_DEVICES {
            return None;
        }
        let base = unsafe { self.ptr.add(HEADER_SIZE) as *mut DeviceSlot };
        Some(unsafe { &mut *base.add(index) })
    }

    fn ring_base(&self, device_index: usize) -> Option<*mut f32> {
        if device_index >= MAX_DEVICES {
            return None;
        }
        let offset = HEADER_SIZE + SLOTS_BYTES + device_index * RING_BYTES_PER_DEVICE;
        Some(unsafe { self.ptr.add(offset) as *mut f32 })
    }

    pub fn write_interleaved(&self, device_index: usize, interleaved: &[f32]) -> usize {
        let Some(base) = self.ring_base(device_index) else {
            return 0;
        };
        let Some(slot) = self.device_slot_mut(device_index) else {
            return 0;
        };
        let frames = interleaved.len() / CHANNELS;
        let mut wi = slot.write_idx as usize;
        for f in 0..frames {
            let dst = (wi % RING_FRAMES) * CHANNELS;
            unsafe {
                *base.add(dst) = interleaved[f * CHANNELS];
                *base.add(dst + 1) = interleaved
                    .get(f * CHANNELS + 1)
                    .copied()
                    .unwrap_or(interleaved[f * CHANNELS]);
            }
            wi = wi.wrapping_add(1);
        }
        slot.write_idx = wi as u32;
        self.header().write_idx.store(wi as u32, Ordering::Release);
        frames
    }

    pub fn read_interleaved(&self, device_index: usize, out: &mut [f32]) -> usize {
        let Some(base) = self.ring_base(device_index) else {
            return 0;
        };
        let Some(slot) = self.device_slot_mut(device_index) else {
            return 0;
        };
        let avail = slot.write_idx.wrapping_sub(slot.read_idx) as usize;
        let want = out.len() / CHANNELS;
        let n = avail.min(want).min(RING_FRAMES);
        let mut ri = slot.read_idx as usize;
        for f in 0..n {
            let src = (ri % RING_FRAMES) * CHANNELS;
            unsafe {
                out[f * CHANNELS] = *base.add(src);
                if f * CHANNELS + 1 < out.len() {
                    out[f * CHANNELS + 1] = *base.add(src + 1);
                }
            }
            ri = ri.wrapping_add(1);
        }
        slot.read_idx = ri as u32;
        self.header().read_idx.store(ri as u32, Ordering::Release);
        n
    }
}

impl Drop for ShmRing {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            if !self.ptr.is_null() {
                libc::munmap(self.ptr as *mut _, self.len);
            }
            if self.fd >= 0 {
                libc::close(self.fd);
            }
        }
    }
}
