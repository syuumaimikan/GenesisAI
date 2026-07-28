use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricCompare {
    pub before: f64,
    pub after: f64,
}

impl MetricCompare {
    pub fn improvement_ratio(&self) -> f64 {
        if self.before == 0.0 {
            return 0.0;
        }
        (self.before - self.after) / self.before
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovementReport {
    pub latency: MetricCompare,
    pub memory_bytes: MetricCompare,
    pub test_pass_rate: f32,
    pub is_eligible_for_merge: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineStage {
    IdentifyingBottleneck,
    GeneratingProposal,
    IsolatedVerification,
    AwaitingApproval,
    Applied,
    Rejected,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovementProposal {
    pub id: Uuid,
    pub target_subsystem: String,
    pub original_commit_sha: String,
    pub proposal_commit_sha: String,
    pub description: String,
    pub stage: PipelineStage,
    pub created_at: SystemTime,
    pub report: Option<ImprovementReport>,
}
