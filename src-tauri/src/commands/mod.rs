use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::client::ApiClient;
use crate::api::types::{DesktopProvisionRequest, DesktopStatusResponse, UserResponse};
use crate::auth::{keychain, oauth, AuthState, AuthStateDto};
use crate::config::{self, AgentConfig, WatchedFolder, API_BASE_URL};
use crate::state::{AgentStatus, AppState};

// In debug builds use a local HTTP server; in release use the custom URL scheme.
#[cfg(debug_assertions)]
const REDIRECT_URI: &str = crate::auth::callback_server::DEV_REDIRECT_URI;
#[cfg(not(debug_assertions))]
const REDIRECT_URI: &str = "https://nordenvault.com/desktop-callback";

// ---- Auth commands ----

#[tauri::command]
pub async fn get_auth_state(app_state: State<'_, AppState>) -> Result<AuthStateDto, String> {
    let auth = app_state.auth.lock().await;
    Ok(auth.to_dto())
}

#[tauri::command]
pub async fn start_login(
    app: AppHandle,
    app_state: State<'_, AppState>,
) -> Result<String, String> {
    let (verifier, challenge) = oauth::generate_pkce();
    let state_str = oauth::generate_state();

    {
        let mut auth = app_state.auth.lock().await;
        auth.pending_pkce = Some(crate::auth::PkceState {
            code_verifier: verifier,
            state: state_str.clone(),
        });
    }

    let client_id = app_state
        .config
        .lock()
        .await
        .workos_client_id
        .clone()
        .ok_or("WorkOS client ID not loaded yet. Please try again in a moment.")?;

    let url = oauth::build_auth_url(&client_id, &challenge, &state_str, REDIRECT_URI);

    // In debug mode, spawn a one-shot HTTP server to capture the callback.
    #[cfg(debug_assertions)]
    {
        let app_for_cb = app.clone();
        tauri::async_runtime::spawn(async move {
            match crate::auth::callback_server::wait_for_callback().await {
                Ok((code, state_param)) => {
                    let state = app_for_cb.state::<AppState>();
                    match do_auth_exchange(code, state_param, &state).await {
                        Ok(_) => {
                            if let Some(win) = app_for_cb.get_webview_window("main") {
                                let _ = win.show();
                                let _ = win.set_focus();
                                let _ = win.emit("auth:complete", ());
                            }
                        }
                        Err(e) => eprintln!("Auth exchange error: {}", e),
                    }
                }
                Err(e) => eprintln!("Callback server error: {}", e),
            }
        });
    }

    Ok(url)
}

/// Called from the deep-link handler in lib.rs (release builds only).
#[tauri::command]
pub async fn handle_auth_callback(
    code: String,
    state_param: String,
    app_state: State<'_, AppState>,
) -> Result<AuthStateDto, String> {
    do_auth_exchange(code, state_param, &app_state).await
}

/// Shared auth exchange logic used by both the callback server (dev) and deep-link (prod).
pub async fn do_auth_exchange(
    code: String,
    state_param: String,
    app_state: &AppState,
) -> Result<AuthStateDto, String> {
    let (verifier, client_id) = {
        let auth = app_state.auth.lock().await;
        let pkce = auth.pending_pkce.as_ref().ok_or("No pending login")?;
        if pkce.state != state_param {
            return Err("State mismatch — possible CSRF".into());
        }
        let client_id = app_state
            .config
            .lock()
            .await
            .workos_client_id
            .clone()
            .ok_or("WorkOS client ID not loaded")?;
        (pkce.code_verifier.clone(), client_id)
    };

    let workos_resp = oauth::exchange_code(&client_id, &code, &verifier, REDIRECT_URI)
        .await
        .map_err(|e| e.to_string())?;

    let client = ApiClient::new(API_BASE_URL.to_string(), app_state.auth.clone());
    let nv_tokens = client
        .post_workos_token(&workos_resp.access_token)
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
pub async fn logout(app_state: State<'_, AppState>) -> Result<(), String> {
    reset_all_state(&app_state).await;
    Ok(())
}

/// Full factory reset: clears tokens, S3 secret, and device registration.
/// Exposed as a command so the UI can offer a "Reset device" recovery option.
#[tauri::command]
pub async fn reset_device(app_state: State<'_, AppState>) -> Result<(), String> {
    reset_all_state(&app_state).await;
    Ok(())
}

async fn reset_all_state(app_state: &AppState) {
    keychain::delete_tokens();
    keychain::delete_s3_secret();
    config::delete();
    let mut auth = app_state.auth.lock().await;
    *auth = AuthState::default();
    let mut cfg = app_state.config.lock().await;
    *cfg = config::AgentConfig::default();
}

// ---- Provisioning ----

#[tauri::command]
pub async fn check_desktop_status(
    app_state: State<'_, AppState>,
) -> Result<DesktopStatusResponse, String> {
    let client = ApiClient::new(API_BASE_URL.to_string(), app_state.auth.clone());
    client
        .get("/api/desktop/status")
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn provision_machine(app_state: State<'_, AppState>) -> Result<AgentConfig, String> {
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

    keychain::store_s3_secret(&resp.secret_access_key).map_err(|e| e.to_string())?;

    let mut cfg = app_state.config.lock().await;
    cfg.machine_id = Some(resp.machine_id);
    cfg.machine_name = Some(resp.machine_name.clone());
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
pub async fn get_config(app_state: State<'_, AppState>) -> Result<AgentConfig, String> {
    Ok(app_state.config.lock().await.clone())
}

#[tauri::command]
pub async fn add_folder(
    path: String,
    app_state: State<'_, AppState>,
) -> Result<AgentConfig, String> {
    let machine_id = {
        let mut cfg = app_state.config.lock().await;
        if !cfg.watched_folders.iter().any(|f| f.path == path) {
            cfg.watched_folders.push(WatchedFolder {
                id: uuid::Uuid::new_v4().to_string(),
                path: path.clone(),
                enabled: true,
            });
            config::save(&cfg).map_err(|e| e.to_string())?;
        }
        cfg.machine_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "unknown".into())
    };

    let queue = app_state.queue.clone();

    // Start filesystem watcher for the new folder
    if let Some(watcher) =
        crate::watcher::start_watcher_for_folder(&machine_id, &path, queue.clone())
    {
        std::mem::forget(watcher);
    }

    // Queue all existing files in the folder for immediate upload
    crate::watcher::scan_existing_files(&machine_id, &path, queue).await;

    Ok(app_state.config.lock().await.clone())
}

#[tauri::command]
pub async fn remove_folder(
    folder_id: String,
    app_state: State<'_, AppState>,
) -> Result<AgentConfig, String> {
    let mut cfg = app_state.config.lock().await;
    cfg.watched_folders.retain(|f| f.id != folder_id);
    config::save(&cfg).map_err(|e| e.to_string())?;
    Ok(cfg.clone())
}

#[tauri::command]
pub async fn toggle_folder(
    folder_id: String,
    enabled: bool,
    app_state: State<'_, AppState>,
) -> Result<AgentConfig, String> {
    let mut cfg = app_state.config.lock().await;
    if let Some(f) = cfg.watched_folders.iter_mut().find(|f| f.id == folder_id) {
        f.enabled = enabled;
    }
    config::save(&cfg).map_err(|e| e.to_string())?;
    Ok(cfg.clone())
}

// ---- Agent status ----

#[tauri::command]
pub async fn get_status(app_state: State<'_, AppState>) -> Result<AgentStatus, String> {
    let mut status = app_state.status.lock().await;
    status.queue_depth = app_state.queue.depth().await;
    Ok(status.clone())
}

#[tauri::command]
pub async fn pause_sync(app_state: State<'_, AppState>) -> Result<(), String> {
    app_state.paused.store(true, Ordering::Relaxed);
    app_state.status.lock().await.status = "paused".into();
    Ok(())
}

#[tauri::command]
pub async fn resume_sync(app_state: State<'_, AppState>) -> Result<(), String> {
    app_state.paused.store(false, Ordering::Relaxed);
    app_state.status.lock().await.status = "idle".into();
    Ok(())
}
