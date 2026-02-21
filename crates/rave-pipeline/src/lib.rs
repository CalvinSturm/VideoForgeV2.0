#![doc = include_str!("../README.md")]

pub mod inference;
pub mod pipeline;

pub use pipeline::{PipelineConfig, PipelineMetrics, UpscalePipeline};
