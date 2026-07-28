// planning/src/manager.rs
use crate::plan::{ExecutionState, ProjectPlan};
use crate::reasoner::Reasoner;
use genesis_core::{EventBus, EventPayload, MessageEnvelope};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct PlanManager {
    current_plan: RwLock<Option<ProjectPlan>>,
    reasoner: Arc<dyn Reasoner>,
    event_bus: EventBus,
}

impl PlanManager {
    pub fn new(reasoner: Arc<dyn Reasoner>, event_bus: EventBus) -> Self {
        Self {
            current_plan: RwLock::new(None),
            reasoner,
            event_bus,
        }
    }

    pub async fn submit_goal(&self, goal: &str) -> Result<(), anyhow::Error> {
        tracing::info!("目標「{}」に対する計画を立案中...", goal);
        let plan = self.reasoner.decompose_goal(goal).await?;

        // 【修正】非同期の.write()呼び出しに.awaitを付与。map_errは不要です。
        let mut plan_guard = self.current_plan.write().await;
        *plan_guard = Some(plan);

        tracing::info!("計画の立案が正常に完了しました。実行フェーズに入ります。");
        Ok(())
    }

    pub async fn start_monitoring_loop(&self) -> Result<(), anyhow::Error> {
        let mut rx = self.event_bus.subscribe();

        while let Ok(msg) = rx.recv().await {
            match msg.payload {
                EventPayload::ActionExecuted {
                    action_id: _,
                    success: false,
                } => {
                    tracing::warn!(
                        "アクション実行の失敗を検知。現状の分析とリプランを実行します。"
                    );
                    self.trigger_replan("依存アクションの実行エラーが発生しました。")
                        .await?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn trigger_replan(&self, reason: &str) -> Result<(), anyhow::Error> {
        // 【修正】非同期の.write()呼び出しに.awaitを付与。map_errは不要です。
        let mut plan_guard = self.current_plan.write().await;

        if let Some(ref current) = *plan_guard {
            let dummy_failed_id = uuid::Uuid::new_v4();

            let revised_plan = self
                .reasoner
                .reflect_and_replan(current, dummy_failed_id, reason)
                .await?;
            *plan_guard = Some(revised_plan);

            tracing::info!("リプランが完了し、新しい計画に置き換わりました。");
        }
        Ok(())
    }
}
