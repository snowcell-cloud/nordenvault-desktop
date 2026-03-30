use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::auth::AuthState;
use crate::config::AgentConfig;
use crate::upload::queue::UploadQueue;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub status: String, // "idle" | "syncing" | "error" | "paused"
    pub last_backup_at: Option<String>,
    pub queue_depth: usize,
    pub error_message: Option<String>,
}

impl Default for AgentStatus {
    fn default() -> Self {
        Self {
            status: "idle".into(),
            last_backup_at: None,
            queue_depth: 0,
            error_message: None,
        }
    }
}

pub struct AppState {
    pub auth: Arc<Mutex<AuthState>>,
    pub config: Arc<Mutex<AgentConfig>>,
    pub queue: Arc<UploadQueue>,
    pub status: Arc<Mutex<AgentStatus>>,
    pub paused: Arc<AtomicBool>,
}
