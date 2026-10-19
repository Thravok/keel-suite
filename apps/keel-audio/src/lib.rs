//! Keel Audio — offline Wave Link alternative (engine host + RPC + FFI for Qt).

pub mod audio_runtime;
pub mod ffi;
pub mod rpc_server;
pub mod service;

pub use service::{AudioService, AudioServiceStart};
