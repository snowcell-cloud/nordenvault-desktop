use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// The API base URL is baked in at compile time.
/// Override with NORDENVAULT_API_URL env var during `cargo build` / `tauri dev`.
pub const API_BASE_URL: &str = match option_env!("NORDENVAULT_API_URL") {
    Some(url) => url,
    None => "https://api.nordenvault.com",
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchedFolder {
    pub id: String,
    pub path: String,
    pub enabled: bool,
}

/// User-editable config persisted to ~/.config/nordenvault/config.json.
/// Does NOT contain secrets (those live in Keychain) or app constants
/// (those are fetched from the backend or baked in at compile time).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    /// Cached WorkOS client ID fetched from /api/desktop/config
    pub workos_client_id: Option<String>,
    pub machine_id: Option<Uuid>,
    pub machine_name: Option<String>,
    pub s3_prefix: Option<String>,
    pub org_id: Option<Uuid>,
    pub credential_id: Option<Uuid>,
    pub bucket_name: Option<String>,
    pub endpoint_url: Option<String>,
    pub region: Option<String>,
    pub access_key_id: Option<String>,
    pub watched_folders: Vec<WatchedFolder>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            workos_client_id: None,
            machine_id: None,
            machine_name: None,
            s3_prefix: None,
            org_id: None,
            credential_id: None,
            bucket_name: None,
            endpoint_url: None,
            region: None,
            access_key_id: None,
            watched_folders: vec![],
        }
    }
}

#[derive(Debug, Deserialize)]
struct RemoteConfigResponse {
    workos_client_id: String,
}

/// Fetch the WorkOS client ID from the backend and cache it in the config.
/// Falls back to the cached value if the request fails (offline support).
pub async fn fetch_and_cache_remote_config(config: &mut AgentConfig) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let resp = client
        .get(format!("{}/api/desktop/config", API_BASE_URL))
        .send()
        .await?
        .error_for_status()?
        .json::<RemoteConfigResponse>()
        .await?;

    config.workos_client_id = Some(resp.workos_client_id);
    save(config)?;
    Ok(())
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nordenvault")
        .join("config.json")
}

pub fn load() -> AgentConfig {
    let path = config_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        AgentConfig::default()
    }
}

/// Delete the on-disk config, effectively un-registering this device.
pub fn delete() {
    let _ = std::fs::remove_file(config_path());
}

pub fn save(config: &AgentConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, data)?;
    Ok(())
}
