//! FFmpeg and file I/O adapters.

pub mod ffmpeg_demuxer;
pub mod ffmpeg_muxer;
pub mod ffmpeg_sys;
pub mod file_sink;
pub mod file_source;
pub mod probe;

pub use probe::{ContainerMetadata, probe_container};
