mod api;
mod auth;
mod commands;
mod config;
mod state;
mod tray;
mod upload;
mod watcher;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

use auth::{keychain, AuthState};
use state::{AgentStatus, AppState};
use upload::queue::UploadQueue;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            // Load persisted config
            let cfg = config::load();

            // Restore tokens from Keychain
            let (access_token, refresh_token) = keychain::load_tokens()
                .map(|(a, r)| (Some(a), Some(r)))
                .unwrap_or((None, None));

            let auth = Arc::new(Mutex::new(AuthState {
                access_token,
                refresh_token,
                ..Default::default()
            }));

            let config_arc = Arc::new(Mutex::new(cfg.clone()));
            let queue = Arc::new(UploadQueue::new());
            let status = Arc::new(Mutex::new(AgentStatus::default()));
            let paused = Arc::new(AtomicBool::new(false));

            // Fetch remote config (workos_client_id) in background, update shared Arc
            let config_for_fetch = config_arc.clone();
            tauri::async_runtime::spawn(async move {
                let mut c = config_for_fetch.lock().await.clone();
                match config::fetch_and_cache_remote_config(&mut c).await {
                    Ok(_) => *config_for_fetch.lock().await = c,
                    Err(e) => eprintln!("Could not fetch remote config (offline?): {}", e),
                }
            });

            // If we restored tokens, fetch the user profile so email/name are available
            let auth_for_profile = auth.clone();
            tauri::async_runtime::spawn(async move {
                let has_token = auth_for_profile.lock().await.access_token.is_some();
                if !has_token {
                    return;
                }
                let client = crate::api::client::ApiClient::new(
                    config::API_BASE_URL.to_string(),
                    auth_for_profile.clone(),
                );
                match client.get::<crate::api::types::UserResponse>("/api/auth/me").await {
                    Ok(user) => {
                        let mut auth = auth_for_profile.lock().await;
                        auth.user_id = Some(user.id);
                        auth.email = Some(user.email);
                        auth.name = user.name;
                    }
                    Err(e) => eprintln!("Could not fetch user profile on startup: {}", e),
                }
            });

            // Start upload worker with cloned Arcs
            let worker_queue = queue.clone();
            let worker_status = status.clone();
            let worker_config = config_arc.clone();
            let worker_paused = paused.clone();

            tauri::async_runtime::spawn(async move {
                upload::worker::run_upload_worker(
                    worker_queue,
                    worker_status,
                    worker_config,
                    worker_paused,
                )
                .await;
            });

            // Start filesystem watchers from current config snapshot
            let watchers = watcher::start_watcher(&cfg, queue.clone());
            std::mem::forget(watchers);

            // Register AppState with Tauri (must use handle in setup)
            app.handle().manage(AppState {
                auth,
                config: config_arc,
                queue,
                status,
                paused,
            });

            // Setup system tray
            tray::setup_tray(app.handle())?;

            // Register deep link URL scheme and listen for open-url events
            #[cfg(target_os = "macos")]
            {
                use tauri::Emitter;
                use tauri_plugin_deep_link::DeepLinkExt;
                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        let url_str = url.to_string();
                        eprintln!("deep-link: {}", url_str);

                        if !url_str.starts_with("nordenvault://auth/callback") {
                            continue;
                        }

                        let parsed = match url::Url::parse(&url_str) {
                            Ok(u) => u,
                            Err(e) => { eprintln!("deep-link parse error: {}", e); continue; }
                        };

                        let mut code = None;
                        let mut state_param = None;
                        for (k, v) in parsed.query_pairs() {
                            match k.as_ref() {
                                "code" => code = Some(v.to_string()),
                                "state" => state_param = Some(v.to_string()),
                                _ => {}
                            }
                        }

                        if let (Some(code), Some(state_param)) = (code, state_param) {
                            let handle = handle.clone();
                            tauri::async_runtime::spawn(async move {
                                let app_state = handle.state::<AppState>();
                                match commands::do_auth_exchange(code, state_param, &app_state).await {
                                    Ok(_) => {
                                        if let Some(win) = handle.get_webview_window("main") {
                                            let _ = win.show();
                                            let _ = win.set_focus();
                                            let _ = win.emit("auth:complete", ());
                                        }
                                    }
                                    Err(e) => eprintln!("Auth exchange error: {}", e),
                                }
                            });
                        }
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_auth_state,
            commands::start_login,
            commands::handle_auth_callback,
            commands::logout,
            commands::reset_device,
            commands::check_desktop_status,
            commands::provision_machine,
            commands::get_config,
            commands::add_folder,
            commands::remove_folder,
            commands::toggle_folder,
            commands::get_status,
            commands::pause_sync,
            commands::resume_sync,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
