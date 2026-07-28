// self_improvement/src/manager.rs
use crate::pipeline::{ImprovementProposal, ImprovementReport, MetricCompare, PipelineStage};
use genesis_core::{EventBus, EventPayload, MessageEnvelope};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct SelfImprovementPipeline {
    active_proposal: RwLock<Option<ImprovementProposal>>,
    event_bus: EventBus,
}

impl SelfImprovementPipeline {
    pub fn new(event_bus: EventBus) -> Self {
        Self {
            active_proposal: RwLock::new(None),
            event_bus,
        }
    }

    pub async fn initiate_improvement(
        &self,
        subsystem: &str,
        current_sha: &str,
        proposed_sha: &str,
        desc: &str,
    ) -> Result<Uuid, anyhow::Error> {
        let proposal = ImprovementProposal {
            id: Uuid::new_v4(),
            target_subsystem: subsystem.to_string(),
            original_commit_sha: current_sha.to_string(),
            proposal_commit_sha: proposed_sha.to_string(),
            description: desc.to_string(),
            stage: PipelineStage::IdentifyingBottleneck,
            created_at: std::time::SystemTime::now(),
            report: None,
        };

        let proposal_id = proposal.id;
        // 【修正】非同期の.write()呼び出しに.awaitを付与。map_errは不要です。
        let mut guard = self.active_proposal.write().await;
        *guard = Some(proposal);

        tracing::info!(
            "自己改善提案を起票しました。ID: {}, 対象: {}",
            proposal_id,
            subsystem
        );
        Ok(proposal_id)
    }

    pub async fn submit_verification_results(
        &self,
        id: Uuid,
        latency_before: f64,
        latency_after: f64,
        mem_before: f64,
        mem_after: f64,
        test_success: bool,
    ) -> Result<(), anyhow::Error> {
        // 【修正】非同期の.write()呼び出しに.awaitを付与。map_errは不要です。
        let mut guard = self.active_proposal.write().await;

        if let Some(ref mut proposal) = *guard {
            if proposal.id != id {
                return Err(anyhow::anyhow!("不整合な提案IDが指定されました。"));
            }

            proposal.stage = PipelineStage::IsolatedVerification;

            let report = ImprovementReport {
                latency: MetricCompare {
                    before: latency_before,
                    after: latency_after,
                },
                memory_bytes: MetricCompare {
                    before: mem_before,
                    after: mem_after,
                },
                test_pass_rate: if test_success { 1.0 } else { 0.0 },
                is_eligible_for_merge: test_success
                    && (latency_before - latency_after) / latency_before >= 0.01,
            };

            proposal.report = Some(report.clone());

            if report.is_eligible_for_merge {
                proposal.stage = PipelineStage::AwaitingApproval;
                tracing::info!(
                    "自動検証に合格しました。人間の承認をお待ちください。遅延削減率: {:.2}%",
                    report.latency.improvement_ratio() * 100.0
                );
                let envelope = MessageEnvelope::new(
                    "SelfImprovementPipeline",
                    EventPayload::TaskScheduled {
                        task_id: format!("APPROVE_IMPROVEMENT_{}", id),
                        priority: 1,
                    },
                );
                let _ = self.event_bus.publish(envelope);
            } else {
                proposal.stage = PipelineStage::Rejected;
                tracing::warn!(
                    "自動検証の品質基準に達しなかったため、この提案は自動却下されました。"
                );
            }
        }
        Ok(())
    }

    pub async fn approve_and_apply(&self, id: Uuid) -> Result<(), anyhow::Error> {
        // 【修正】非同期の.write()呼び出しに.awaitを付与。map_errは不要です。
        let mut guard = self.active_proposal.write().await;

        if let Some(ref mut proposal) = *guard {
            if proposal.id == id && proposal.stage == PipelineStage::AwaitingApproval {
                proposal.stage = PipelineStage::Applied;
                tracing::info!(
                    "人間の管理者によって承認されました。変更をブランチにマージします。対象: {}",
                    proposal.target_subsystem
                );
                return Ok(());
            }
        }
        Err(anyhow::anyhow!("承認不可能な状態、またはID不一致です。"))
    }

    pub async fn rollback_due_to_regression(&self, id: Uuid) -> Result<(), anyhow::Error> {
        // 【修正】非同期の.write()呼び出しに.awaitを付与。map_errは不要です。
        let mut guard = self.active_proposal.write().await;

        if let Some(ref mut proposal) = *guard {
            if proposal.id == id && proposal.stage == PipelineStage::Applied {
                proposal.stage = PipelineStage::RolledBack;
                tracing::error!(
                    "【ロールバック】変更統合後に性能劣化が検知されたため、コミット '{}' に差し戻しました。",
                    proposal.original_commit_sha
                );
                return Ok(());
            }
        }
        Err(anyhow::anyhow!("ロールバックを実行できません。"))
    }
}
