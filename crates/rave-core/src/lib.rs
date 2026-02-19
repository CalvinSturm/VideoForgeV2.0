//! # rave-core — Foundation types for RAVE
//!
//! Provides the shared type vocabulary, GPU context, error hierarchy,
//! backend trait, codec traits, and FFI type aliases used by all RAVE crates.
//! This crate has zero NVDEC/NVENC/FFmpeg dependencies — only `cudarc`.

pub mod backend;
pub mod codec_traits;
pub mod context;
pub mod debug_alloc;
pub mod error;
pub mod ffi_types;
pub mod types;
