//! Pipeline orchestration.

pub mod inference;
pub mod pipeline;

pub use pipeline::{PipelineConfig, PipelineMetrics, UpscalePipeline};
