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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BackupMode {
    #[default]
    Unselected,
    Private,
    Standard,
}

/// User-editable config persisted to ~/.config/nordenvault/config.json.
/// Does NOT contain secrets (those live in Keychain) or app constants
/// (those are fetched from the backend or baked in at compile time).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    #[serde(default)]
    pub encrypted_repository: Option<String>,
    #[serde(default)]
    pub backup_mode: BackupMode,
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
            encrypted_repository: None,
            backup_mode: BackupMode::Unselected,
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

impl AgentConfig {
    pub fn choose_standard_backup(&mut self) {
        if self.backup_mode == BackupMode::Private {
            // Require a fresh folder selection before exposing any formerly private data.
            self.watched_folders.clear();
        }
        self.backup_mode = BackupMode::Standard;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_and_privacy_downgrade_require_explicit_choices() {
        let mut cfg: AgentConfig = serde_json::from_value(serde_json::json!({
            "watchedFolders":[{"id":"old","path":"/old","enabled":true}]
        }))
        .unwrap();
        assert_eq!(cfg.backup_mode, BackupMode::Unselected);
        cfg.choose_standard_backup();
        assert_eq!(cfg.watched_folders.len(), 1);
        cfg.backup_mode = BackupMode::Private;
        cfg.encrypted_repository = Some("repository-id".into());
        cfg.choose_standard_backup();
        assert!(cfg.watched_folders.is_empty());
        assert_eq!(cfg.backup_mode, BackupMode::Standard);
        assert!(cfg.encrypted_repository.is_some());
    }
}
