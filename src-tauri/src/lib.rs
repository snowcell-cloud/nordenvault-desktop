mod api;
mod auth;
mod commands;
mod config;
mod state;
mod tray;
mod backup;
mod standard;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

use auth::{keychain, AuthState};
use state::{AgentStatus, AppState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            let status = Arc::new(Mutex::new(AgentStatus::default()));
            let paused = Arc::new(AtomicBool::new(false));

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

            let state = Arc::new(AppState {
                auth, config: config_arc, status, paused,
                backup_operation: Mutex::new(()),
                wake_backup: tokio::sync::Notify::new(),
            });
            app.manage(state.clone());
            tauri::async_runtime::spawn(backup::run(state));

            // Check for updates in the background (release builds only)
            #[cfg(not(debug_assertions))]
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    use tauri::Emitter;
                    use tauri_plugin_updater::UpdaterExt;
                    match handle.updater() {
                        Ok(updater) => match updater.check().await {
                            Ok(Some(update)) => {
                                eprintln!("[updater] New version available: {}", update.version);
                                if let Some(win) = handle.get_webview_window("main") {
                                    let _ = win.emit("update:available", &update.version);
                                }
                            }
                            Ok(None) => eprintln!("[updater] App is up to date"),
                            Err(e) => eprintln!("[updater] Check failed: {e}"),
                        },
                        Err(e) => eprintln!("[updater] Could not get updater: {e}"),
                    }
                });
            }

            // Hide window on close instead of quitting — app lives in the menu bar
            if let Some(main_window) = app.get_webview_window("main") {
                let win = main_window.clone();
                main_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });
            }

            // Setup system tray
            tray::setup_tray(app.handle())?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            backup::enable_standard_backup,
            backup::generate_recovery_key,
            backup::enable_encrypted_backup,
            backup::backup_snapshots,
            backup::restore_snapshot,
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

#[cfg(all(test, target_os = "linux"))]
mod glib_security_regression {
    use glib::variant::ToVariant;
    #[test]
    fn variant_string_iteration_has_valid_forward_and_reverse_pointers() {
        let values = vec!["first", "second", "third"];
        let variant = values.to_variant();
        assert_eq!(variant.array_iter_str().unwrap().collect::<Vec<_>>(), values);
        assert_eq!(variant.array_iter_str().unwrap().rev().collect::<Vec<_>>(), vec!["third", "second", "first"]);
    }
}
