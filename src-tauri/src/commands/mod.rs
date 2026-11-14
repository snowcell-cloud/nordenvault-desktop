use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::client::ApiClient;
use crate::api::types::{DesktopProvisionRequest, DesktopStatusResponse, UserResponse};
use crate::auth::{keychain, oauth, AuthState, AuthStateDto};
use crate::config::{self, AgentConfig, WatchedFolder, API_BASE_URL};
use crate::state::{AgentStatus, AppState};

use crate::auth::callback_server::{wait_for_callback, REDIRECT_URI};

// ---- Auth commands ----

#[tauri::command]
pub async fn get_auth_state(app_state: State<'_, Arc<AppState>>) -> Result<AuthStateDto, String> {
    let auth = app_state.auth.lock().await;
    Ok(auth.to_dto())
}

#[tauri::command]
pub async fn start_login(
    app: AppHandle,
    app_state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 51234))
        .await
        .map_err(|_| "Another sign-in is already using the local callback. Please try again.".to_string())?;
    let (verifier, challenge) = oauth::generate_pkce();
    let state_str = oauth::generate_state();

    {
        let mut auth = app_state.auth.lock().await;
        auth.pending_pkce = Some(crate::auth::PkceState {
            code_verifier: verifier,
            state: state_str.clone(),
        });
    }

    let url = oauth::build_auth_url(&challenge, &state_str, REDIRECT_URI);

    tauri::async_runtime::spawn(async move {
        let result = tokio::time::timeout(std::time::Duration::from_secs(300), async {
            let code = wait_for_callback(listener, &state_str).await.map_err(|e| e.to_string())?;
            do_auth_exchange(code, state_str, &app.state::<Arc<AppState>>()).await
        }).await;
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.show();
            let _ = win.set_focus();
            match result {
                Ok(Ok(_)) => { let _ = win.emit("auth:complete", ()); }
                _ => { let _ = win.emit("auth:error", "Sign-in could not be completed. Please try again."); }
            }
        }
    });

    Ok(url)
}

/// Called from the deep-link handler in lib.rs (release builds only).
#[tauri::command]
pub async fn handle_auth_callback(
    code: String,
    state_param: String,
    app_state: State<'_, Arc<AppState>>,
) -> Result<AuthStateDto, String> {
    do_auth_exchange(code, state_param, &app_state).await
}

/// Shared auth exchange logic used by both the callback server (dev) and deep-link (prod).
pub async fn do_auth_exchange(
    code: String,
    state_param: String,
    app_state: &AppState,
) -> Result<AuthStateDto, String> {
    let verifier = {
        let auth = app_state.auth.lock().await;
        let pkce = auth.pending_pkce.as_ref().ok_or("No pending login")?;
        if pkce.state != state_param {
            return Err("State mismatch — possible CSRF".into());
        }
        pkce.code_verifier.clone()
    };

    let identity = oauth::exchange_code(&code, &verifier, REDIRECT_URI)
        .await
        .map_err(|e| e.to_string())?;

    let client = ApiClient::new(API_BASE_URL.to_string(), app_state.auth.clone());
    let nv_tokens = client
        .post_identity_token(&identity.access_token)
        .await
        .map_err(|e| e.to_string())?;

    keychain::store_tokens(&nv_tokens.access_token, &nv_tokens.refresh_token)
        .map_err(|e| e.to_string())?;

    {
        let mut auth = app_state.auth.lock().await;
        auth.access_token = Some(nv_tokens.access_token.clone());
        auth.refresh_token = Some(nv_tokens.refresh_token.clone());
        auth.pending_pkce = None;
    }

    let user: UserResponse = client.get("/api/auth/me").await.map_err(|e| e.to_string())?;

    {
        let mut auth = app_state.auth.lock().await;
        auth.user_id = Some(user.id);
        auth.email = Some(user.email);
        auth.name = user.name;
    }

    Ok(app_state.auth.lock().await.to_dto())
}

#[tauri::command]
pub async fn logout(app_state: State<'_, Arc<AppState>>) -> Result<(), String> {
    reset_all_state(&app_state).await
}

/// Full factory reset: clears tokens, S3 secret, and device registration.
/// Exposed as a command so the UI can offer a "Reset device" recovery option.
#[tauri::command]
pub async fn reset_device(app_state: State<'_, Arc<AppState>>) -> Result<(), String> {
    reset_all_state(&app_state).await
}

async fn reset_all_state(app_state: &AppState) -> Result<(), String> {
    let _operation = app_state.backup_operation.lock().await;
    // Remove this device’s saved recovery key only on explicit sign-out/reset.
    // The user confirmed an independent copy before the first backup.
    let cfg = app_state.config.lock().await.clone();
    if cfg.encrypted_repository.is_some() { crate::backup::forget_saved_key(&cfg)?; }
    keychain::delete_tokens();
    keychain::delete_s3_secret();
    config::delete();
    *app_state.auth.lock().await = AuthState::default();
    let mut cfg = app_state.config.lock().await;
    *cfg = config::AgentConfig::default();
    *app_state.status.lock().await = AgentStatus::default();
    app_state.paused.store(false, Ordering::Relaxed);
    app_state.wake_backup.notify_one();
    Ok(())
}

// ---- Provisioning ----

#[tauri::command]
pub async fn check_desktop_status(
    app_state: State<'_, Arc<AppState>>,
) -> Result<DesktopStatusResponse, String> {
    let client = ApiClient::new(API_BASE_URL.to_string(), app_state.auth.clone());
    client
        .get("/api/desktop/status")
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn provision_machine(app_state: State<'_, Arc<AppState>>) -> Result<AgentConfig, String> {
    let _operation = app_state.backup_operation.lock().await;
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".into());

    let client = ApiClient::new(API_BASE_URL.to_string(), app_state.auth.clone());

    let body = DesktopProvisionRequest {
        hostname,
        platform: std::env::consts::OS.to_string(),
    };
    let resp: crate::api::types::DesktopProvisionResponse = client
        .post("/api/desktop/provision", &body)
        .await
        .map_err(|e| e.to_string())?;

    let mut cfg = app_state.config.lock().await;
    if resp.secret_access_key == "(already provisioned)" {
        let same_registration = cfg.org_id == Some(resp.org_id)
            && cfg.machine_id == Some(resp.machine_id)
            && cfg.credential_id == Some(resp.credential_id)
            && cfg.access_key_id.as_deref() == Some(resp.access_key_id.as_str());
        let has_secret = keychain::load_s3_secret()
            .is_some_and(|secret| !secret.is_empty() && secret != "(already provisioned)");
        if !same_registration || !has_secret {
            return Err("This device is already registered, but its saved backup key is unavailable. Contact NordenVault support to reconnect it.".into());
        }
    } else {
        keychain::store_s3_secret(&resp.secret_access_key).map_err(|e| e.to_string())?;
    }
    if cfg.org_id != Some(resp.org_id) || cfg.machine_id != Some(resp.machine_id) {
        *cfg = AgentConfig::default();
    }
    cfg.machine_id = Some(resp.machine_id);
    cfg.machine_name = Some(resp.machine_name.clone());
    cfg.s3_prefix = Some(resp.s3_prefix.clone());
    cfg.org_id = Some(resp.org_id);
    cfg.credential_id = Some(resp.credential_id);
    cfg.bucket_name = Some(resp.bucket_name);
    cfg.endpoint_url = Some(resp.endpoint_url);
    cfg.region = Some(resp.region);
    cfg.access_key_id = Some(resp.access_key_id);

    config::save(&cfg).map_err(|e| e.to_string())?;
    Ok(cfg.clone())
}

// ---- Config / folders ----

#[tauri::command]
pub async fn get_config(app_state: State<'_, Arc<AppState>>) -> Result<AgentConfig, String> {
    Ok(app_state.config.lock().await.clone())
}

#[tauri::command]
pub async fn add_folder(
    path: String,
    app_state: State<'_, Arc<AppState>>,
) -> Result<AgentConfig, String> {
    let mut cfg = app_state.config.lock().await;
    if cfg.backup_mode == config::BackupMode::Unselected { return Err("Choose a backup mode before adding folders.".into()); }
    if !std::path::Path::new(&path).is_absolute() || !std::path::Path::new(&path).is_dir() { return Err("Select a folder on this computer.".into()); }
    if !cfg.watched_folders.iter().any(|f| f.path == path) {
        cfg.watched_folders.push(WatchedFolder { id: uuid::Uuid::new_v4().to_string(), path, enabled: true });
        config::save(&cfg).map_err(|e| e.to_string())?;
    }
    app_state.wake_backup.notify_one();
    Ok(cfg.clone())
}

#[tauri::command]
pub async fn remove_folder(
    folder_id: String,
    app_state: State<'_, Arc<AppState>>,
) -> Result<AgentConfig, String> {
    let mut cfg = app_state.config.lock().await;
    cfg.watched_folders.retain(|f| f.id != folder_id);
    app_state.wake_backup.notify_one();
    config::save(&cfg).map_err(|e| e.to_string())?;
    Ok(cfg.clone())
}

#[tauri::command]
pub async fn toggle_folder(
    folder_id: String,
    enabled: bool,
    app_state: State<'_, Arc<AppState>>,
) -> Result<AgentConfig, String> {
    let mut cfg = app_state.config.lock().await;
    if let Some(f) = cfg.watched_folders.iter_mut().find(|f| f.id == folder_id) {
        f.enabled = enabled;
        app_state.wake_backup.notify_one();
    }
    config::save(&cfg).map_err(|e| e.to_string())?;
    Ok(cfg.clone())
}

// ---- Agent status ----

#[tauri::command]
pub async fn get_status(app_state: State<'_, Arc<AppState>>) -> Result<AgentStatus, String> {
    Ok(app_state.status.lock().await.clone())
}

#[tauri::command]
pub async fn pause_sync(app_state: State<'_, Arc<AppState>>) -> Result<(), String> {
    app_state.paused.store(true, Ordering::Relaxed);
    app_state.status.lock().await.status = "paused".into();
    Ok(())
}

#[tauri::command]
pub async fn resume_sync(app_state: State<'_, Arc<AppState>>) -> Result<(), String> {
    app_state.paused.store(false, Ordering::Relaxed);
    app_state.status.lock().await.status = "idle".into();
    app_state.wake_backup.notify_one();
    Ok(())
}
