// planning/src/reasoner.rs
use crate::plan::{ExecutionState, Milestone, ProjectPlan};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[async_trait]
pub trait Reasoner: Send + Sync {
    async fn decompose_goal(&self, goal: &str) -> Result<ProjectPlan, anyhow::Error>;
    async fn reflect_and_replan(
        &self,
        current_plan: &ProjectPlan,
        failed_task_id: uuid::Uuid,
        error_context: &str,
    ) -> Result<ProjectPlan, anyhow::Error>;
}

pub struct OllamaReasoner {
    client: reqwest::Client,
    endpoint: String,
    model: String,
}

impl OllamaReasoner {
    pub fn new(endpoint: &str, model: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint: endpoint.to_string(),
            model: model.to_string(),
        }
    }
}

#[derive(Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    format: String, // "json" を指定して構造化出力を強制
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

#[async_trait]
impl Reasoner for OllamaReasoner {
    async fn decompose_goal(&self, goal: &str) -> Result<ProjectPlan, anyhow::Error> {
        let prompt = format!(
            "あなたは自律型OS 'GenesisAI' の極秘計画モジュールです。\n\
            以下の目標（Goal）を分析し、マイルストーンに分解して、必ず以下の構造のJSONオブジェクトだけを返却してください。\n\
            お喋りやJSON以外の説明は一切禁止します。\n\n\
            構造:\n\
            {{\n\
              \"id\": \"<ランダムなUUID>\",\n\
              \"goal\": \"{}\",\n\
              \"milestones\": [\n\
                {{\n\
                  \"id\": \"<ランダムなUUID>\",\n\
                  \"title\": \"マイルストーンの具体的なタイトル\",\n\
                  \"tasks\": [],\n\
                  \"state\": \"Pending\"\n\
                }}\n\
              ],\n\
              \"state\": \"InProgress\"\n\
            }}\n\n\
            目標: {}",
            goal, goal
        );

        let request = OllamaGenerateRequest {
            model: self.model.clone(),
            prompt,
            format: "json".to_string(),
            stream: false,
        };

        let res = self
            .client
            .post(&format!("{}/api/generate", self.endpoint))
            .json(&request)
            .send()
            .await?;

        let body: OllamaGenerateResponse = res.json().await?;

        // 得られたJSON文字列を ProjectPlan にデシリアライズ
        let plan: ProjectPlan = serde_json::from_str(&body.response)?;
        Ok(plan)
    }

    async fn reflect_and_replan(
        &self,
        current_plan: &ProjectPlan,
        _failed_task_id: uuid::Uuid,
        error_context: &str,
    ) -> Result<ProjectPlan, anyhow::Error> {
        let prompt = format!(
            "現在の計画において失敗が検知されました。\n\
            現在の計画: {:?}\n\
            エラーコンテキスト: {}\n\n\
            このエラーを回避・修復するための、修正されたマイルストーン計画をJSONで生成してください。形式は以前と同様です。",
            current_plan, error_context
        );

        let request = OllamaGenerateRequest {
            model: self.model.clone(),
            prompt,
            format: "json".to_string(),
            stream: false,
        };

        let res = self
            .client
            .post(&format!("{}/api/generate", self.endpoint))
            .json(&request)
            .send()
            .await?;

        let body: OllamaGenerateResponse = res.json().await?;
        let plan: ProjectPlan = serde_json::from_str(&body.response)?;
        Ok(plan)
    }
}
