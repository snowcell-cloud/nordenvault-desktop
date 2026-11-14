//! The storage service receives only restic repository objects. The random
//! repository password is created locally and retained in the OS credential store.
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::State;
use tokio::process::Command;

use crate::{
    auth::keychain,
    config::{self, AgentConfig, BackupMode},
    state::AppState,
};

fn generate_key() -> String {
    let mut bytes = [0; 32];
    OsRng.fill_bytes(&mut bytes);
    format!("NV1-{}", URL_SAFE_NO_PAD.encode(bytes))
}

fn repository_id(key: &str) -> Result<String> {
    let value = key
        .strip_prefix("NV1-")
        .context("Enter the complete NordenVault recovery key.")?;
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .ok()
        .filter(|b| b.len() == 32)
        .context("The recovery key is incomplete or invalid.")?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn entry(cfg: &AgentConfig) -> Result<keyring::Entry> {
    let org = cfg.org_id.context("Register this device first.")?;
    let machine = cfg.machine_id.context("Register this device first.")?;
    Ok(keyring::Entry::new(
        "com.nordenvault.desktop.backup",
        &format!("{org}/{machine}"),
    )?)
}

pub fn forget_saved_key(cfg: &AgentConfig) -> Result<(), String> {
    let store = entry(cfg).map_err(|e| e.to_string())?;
    match store.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("Unlock your system credential store to finish signing out.".into()),
    }
}

fn saved_key(cfg: &AgentConfig) -> Result<String> {
    let key = entry(cfg)?
        .get_password()
        .context("Unlock your system credential store or enter your saved recovery key.")?;
    ensure!(
        Some(&repository_id(&key)?) == cfg.encrypted_repository.as_ref(),
        "The saved recovery key does not match this device."
    );
    Ok(key)
}

struct Repository {
    location: String,
    key: String,
    access: String,
    secret: String,
    region: String,
}

impl Repository {
    fn new(cfg: &AgentConfig, key: String) -> Result<Self> {
        let endpoint = cfg
            .endpoint_url
            .as_deref()
            .context("Missing storage endpoint.")?;
        let url = url::Url::parse(endpoint)?;
        ensure!(
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "A secure storage endpoint is required."
        );
        let bucket = cfg.bucket_name.as_deref().context("Missing bucket.")?;
        ensure!(
            !bucket.is_empty()
                && bucket
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b)),
            "Invalid bucket."
        );
        let location = format!(
            "s3:{}/{bucket}/desktop-encrypted/v1/{}",
            endpoint.trim_end_matches('/'),
            repository_id(&key)?
        );
        Ok(Self {
            location,
            key,
            access: cfg
                .access_key_id
                .clone()
                .context("Missing storage credential.")?,
            secret: keychain::load_s3_secret().context("Unlock the device storage credential.")?,
            region: cfg.region.clone().unwrap_or_default(),
        })
    }

    fn command(&self) -> Result<Command> {
        // Tauri installs externalBin next to the application executable on all desktop targets.
        let mut directory = std::env::current_exe()?;
        directory.pop();
        #[cfg(test)]
        if directory.ends_with("deps") {
            directory.pop();
        }
        let bin = directory.join(if cfg!(windows) {
            "nordenvault-restic.exe"
        } else {
            "nordenvault-restic"
        });
        ensure!(
            bin.is_file(),
            "Bundled backup engine is missing. Reinstall NordenVault."
        );
        let mut cmd = Command::new(bin);
        for (name, _) in std::env::vars_os() {
            let upper = name.to_string_lossy().to_ascii_uppercase();
            if upper.starts_with("RESTIC_") || upper.starts_with("AWS_") {
                cmd.env_remove(name);
            }
        }
        cmd.env("RESTIC_REPOSITORY", &self.location)
            .env("RESTIC_PASSWORD", &self.key)
            .env("AWS_ACCESS_KEY_ID", &self.access)
            .env("AWS_SECRET_ACCESS_KEY", &self.secret)
            .args(["--no-cache", "-o", "s3.bucket-lookup=path"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if !self.region.is_empty() {
            cmd.arg("-o").arg(format!("s3.region={}", self.region));
        }
        Ok(cmd)
    }

    async fn open(&self, create: bool) -> Result<()> {
        let status = self
            .command()?
            .args(["cat", "config"])
            .stdout(Stdio::null())
            .status()
            .await?;
        if status.success() {
            return Ok(());
        }
        if create && status.code() == Some(10) {
            let created = self
                .command()?
                .args(["init", "--repository-version", "2"])
                .stdout(Stdio::null())
                .status()
                .await?;
            ensure!(
                created.success(),
                "Could not create the encrypted repository. Check storage access and try again."
            );
            return Ok(());
        }
        bail!("Could not open this repository. Check the recovery key, organisation and storage connection.")
    }

    async fn backup(&self, folders: &[PathBuf]) -> Result<()> {
        let status = self
            .command()?
            .args([
                "backup",
                "--quiet",
                "--skip-if-unchanged",
                "--exclude-caches",
                "--host",
                "desktop",
                "--",
            ])
            .args(folders)
            .stdout(Stdio::null())
            .status()
            .await?;
        ensure!(status.success(), "Encrypted backup did not complete (code {:?}). Check the network, storage quota and access to the selected folders.",status.code());
        Ok(())
    }

    async fn snapshots(&self) -> Result<Vec<Snapshot>> {
        let out = self
            .command()?
            .args(["--no-lock", "snapshots", "--json"])
            .output()
            .await?;
        ensure!(
            out.status.success(),
            "Could not read backups. Check the key and storage access."
        );
        let mut list: Vec<Snapshot> = serde_json::from_slice(&out.stdout)?;
        list.sort_by(|a, b| b.time.cmp(&a.time));
        Ok(list)
    }

    async fn restore(&self, snapshot: &str, destination: &Path) -> Result<PathBuf> {
        ensure!(
            snapshot.len() == 64 && snapshot.bytes().all(|b| b.is_ascii_hexdigit()),
            "Select a snapshot first."
        );
        ensure!(
            destination.is_dir(),
            "Select an existing destination folder."
        );
        let target = destination.join(format!("NordenVault restore {}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&target)?;
        let result = self
            .command()?
            .args(["--no-lock", "restore", snapshot, "--verify", "--target"])
            .arg(&target)
            .stdout(Stdio::null())
            .status()
            .await?;
        ensure!(
            result.success(),
            "Restore did not complete. Partial files may be in {}.",
            target.display()
        );
        Ok(target)
    }
}

#[derive(Serialize, serde::Deserialize)]
pub struct Snapshot {
    id: String,
    time: String,
}

#[tauri::command]
pub fn generate_recovery_key() -> String {
    generate_key()
}

#[tauri::command]
pub async fn enable_encrypted_backup(
    key: String,
    create: bool,
    state: State<'_, Arc<AppState>>,
) -> Result<AgentConfig, String> {
    async {
        let _operation = state.backup_operation.lock().await;
        let cfg = state.config.lock().await.clone();
        let key = key.trim().to_owned();
        let id = repository_id(&key)?;
        ensure!(
            cfg.encrypted_repository
                .as_ref()
                .map_or(true, |old| old == &id),
            "Use the recovery key already assigned to this device."
        );
        let store = entry(&cfg)?;
        let repo = Repository::new(&cfg, key)?;
        repo.open(create && cfg.encrypted_repository.is_none())
            .await?;
        store.set_password(&repo.key).context(
            "Could not save the key in the system credential store. Unlock it and retry.",
        )?;
        let mut cfg = state.config.lock().await;
        let mut next = cfg.clone();
        next.encrypted_repository = Some(id);
        next.backup_mode = BackupMode::Private;
        config::save(&next)?;
        *cfg = next.clone();
        state.wake_backup.notify_one();
        Ok(next)
    }
    .await
    .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
pub async fn enable_standard_backup(
    state: State<'_, Arc<AppState>>,
) -> Result<AgentConfig, String> {
    let _operation = state.backup_operation.lock().await;
    let mut cfg = state.config.lock().await;
    if cfg.machine_id.is_none() {
        return Err("Register this device first.".into());
    }
    let mut next = cfg.clone();
    next.choose_standard_backup();
    config::save(&next).map_err(|e| e.to_string())?;
    *cfg = next.clone();
    *state.status.lock().await = Default::default();
    state.wake_backup.notify_one();
    Ok(next)
}

#[tauri::command]
pub async fn backup_snapshots(
    key: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<Snapshot>, String> {
    let cfg = state.config.lock().await.clone();
    async {
        Repository::new(&cfg, key.trim().to_owned())?
            .snapshots()
            .await
    }
    .await
    .map_err(|e: anyhow::Error| e.to_string())
}

#[tauri::command]
pub async fn restore_snapshot(
    key: String,
    snapshot: String,
    destination: PathBuf,
    state: State<'_, Arc<AppState>>,
) -> Result<PathBuf, String> {
    let _op = state.backup_operation.lock().await;
    let cfg = state.config.lock().await.clone();
    async {
        Repository::new(&cfg, key.trim().to_owned())?
            .restore(&snapshot, &destination)
            .await
    }
    .await
    .map_err(|e: anyhow::Error| e.to_string())
}

async fn snapshot_cycle(
    state: &AppState,
    standard: &mut crate::standard::StandardBackup,
) -> Result<()> {
    let _operation = state.backup_operation.lock().await;
    if state.paused.load(Ordering::Relaxed) || state.auth.lock().await.access_token.is_none() {
        return Ok(());
    }
    let cfg = state.config.lock().await.clone();
    let folders: Vec<_> = cfg
        .watched_folders
        .iter()
        .filter(|f| f.enabled)
        .map(|f| PathBuf::from(&f.path))
        .collect();
    if folders.is_empty() || cfg.backup_mode == BackupMode::Unselected {
        return Ok(());
    }
    state.status.lock().await.status = "syncing".into();
    match cfg.backup_mode {
        BackupMode::Private => {
            standard.clear();
            let key = saved_key(&cfg)?;
            Repository::new(&cfg, key)?.backup(&folders).await?;
        }
        BackupMode::Standard => standard.backup(&cfg).await?,
        BackupMode::Unselected => unreachable!(),
    }
    let mut status = state.status.lock().await;
    status.status = if state.paused.load(Ordering::Relaxed) {
        "paused"
    } else {
        "idle"
    }
    .into();
    status.last_backup_at = Some(chrono::Utc::now().to_rfc3339());
    status.error_message = None;
    Ok(())
}

pub async fn run(state: Arc<AppState>) {
    let wake = Arc::downgrade(&state);
    let watcher = notify_debouncer_mini::new_debouncer(
        Duration::from_secs(3),
        move |events: notify_debouncer_mini::DebounceEventResult| {
            if let Ok(events) = events {
                if !events.is_empty() {
                    if let Some(state) = wake.upgrade() {
                        state.wake_backup.notify_one();
                    }
                }
            }
        },
    );
    // A periodic scan also covers missed notifications and unavailable watch handles.
    let mut watcher = watcher.ok();
    let mut watching = std::collections::HashSet::<PathBuf>::new();
    let mut standard = crate::standard::StandardBackup::default();
    loop {
        let cfg = state.config.lock().await.clone();
        let selected: std::collections::HashSet<_> = cfg
            .watched_folders
            .iter()
            .filter(|f| cfg.backup_mode != BackupMode::Unselected && f.enabled)
            .map(|f| PathBuf::from(&f.path))
            .collect();
        if let Some(watcher) = watcher.as_mut() {
            watching.retain(|p| {
                if selected.contains(p) {
                    true
                } else {
                    let _ = watcher.watcher().unwatch(p);
                    false
                }
            });
            for path in selected {
                if !watching.contains(&path)
                    && watcher
                        .watcher()
                        .watch(
                            &path,
                            notify_debouncer_mini::notify::RecursiveMode::Recursive,
                        )
                        .is_ok()
                {
                    watching.insert(path);
                }
            }
        }
        if let Err(e) = snapshot_cycle(&state, &mut standard).await {
            let mut status = state.status.lock().await;
            status.status = "error".into();
            status.error_message = Some(e.to_string());
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(300)) => {},
            _ = state.wake_backup.notified() => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn encrypted_snapshot_restores_with_only_saved_key_and_repository() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let source = dir.path().join("private-folder-marker");
        fs::create_dir(&source)?;
        let name = "confidential-filename-marker.txt";
        let content = b"confidential-content-marker";
        fs::write(source.join(name), content)?;
        let storage = tempfile::tempdir()?;
        let location = storage.path().join("repo").to_str().unwrap().to_owned();
        let recovery = generate_key();
        let open = |key| Repository {
            location: location.clone(),
            key,
            access: String::new(),
            secret: String::new(),
            region: String::new(),
        };
        let writer = open(recovery.clone());
        writer.open(true).await?;
        writer.backup(std::slice::from_ref(&source)).await?;
        let snapshots = writer.snapshots().await?;
        assert_eq!(snapshots.len(), 1);
        fs::remove_dir_all(source)?;
        let wrong = open(generate_key());
        assert!(wrong.open(false).await.is_err());
        assert!(wrong.snapshots().await.is_err());
        let recovered = open(recovery.clone())
            .restore(&snapshots[0].id, dir.path())
            .await?;
        fn files(path: &Path, found: &mut Vec<PathBuf>) -> Result<()> {
            for entry in fs::read_dir(path)? {
                let p = entry?.path();
                if p.is_dir() {
                    files(&p, found)?
                } else {
                    found.push(p)
                }
            }
            Ok(())
        }
        let mut restored = vec![];
        files(&recovered, &mut restored)?;
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].file_name().unwrap(), name);
        assert_eq!(fs::read(&restored[0])?, content);
        let mut objects = vec![];
        files(Path::new(&location), &mut objects)?;
        for p in objects {
            let data = fs::read(&p)?;
            for marker in [
                name.as_bytes(),
                content,
                recovery.as_bytes(),
                b"private-folder-marker",
            ] {
                assert!(!data.windows(marker.len()).any(|w| w == marker));
                assert!(!p
                    .strip_prefix(&location)?
                    .to_string_lossy()
                    .contains(std::str::from_utf8(marker)?));
            }
        }
        Ok(())
    }
}
