pub mod manager;
pub mod pipeline;

pub use manager::SelfImprovementPipeline;
pub use pipeline::{ImprovementProposal, ImprovementReport, MetricCompare, PipelineStage};
