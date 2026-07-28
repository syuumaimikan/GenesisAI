use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionState {
    Pending,
    InProgress,
    Completed,
    Failed,
    Suspended,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomicAction {
    pub id: Uuid,
    pub name: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub state: ExecutionState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub subtasks: Vec<Task>,
    pub actions: Vec<AtomicAction>,
    pub state: ExecutionState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub id: Uuid,
    pub title: String,
    pub tasks: Vec<Task>,
    pub state: ExecutionState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectPlan {
    pub id: Uuid,
    pub goal: String,
    pub milestones: Vec<Milestone>,
    pub state: ExecutionState,
}
