use serde::{Deserialize, Serialize};

/// A task; `id` is given by the back and is ignored when a task is added.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Task {
    #[serde(default)]
    pub id: i64,
    pub description: String,
    #[serde(default)]
    pub completed: bool,
}

/// The answer of the back, as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum Response {
    List(Vec<Task>),
    Error(String),
}
