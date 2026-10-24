pub mod agents_seed;
pub mod app_state;
pub mod chat_commands;
pub mod commands;
pub mod config;
pub mod host_process;
pub mod lan;
pub mod notifications;
pub mod pairing_commands;

use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use app_state::{lock, AppState};

pub fn run() {
    let app = match tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state = Arc::new(AppState::new(app.handle()));
            app.manage(state.clone());
            if lock(&state.inner).config.local {
                state.start_local_host(app.handle().clone());
            }
            let handle = app.handle().clone();
            let registered = app.notification().on_action(move |performed| {
                if performed.action_id() != "tap" {
                    return;
                }
                if let Some(window) = handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                let pane_id = performed
                    .notification()
                    .and_then(|notification| notification.extra().get("pane_id"))
                    .and_then(|value| value.as_str())
                    .map(str::to_string);
                if let Some(pane_id) = pane_id {
                    let _ = handle.emit("hn-notification-click", pane_id);
                }
            });
            if let Err(error) = registered {
                eprintln!("los avisos no podrán abrir su agente: {error}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::connect,
            commands::pair,
            commands::save_advanced,
            commands::refresh_state,
            commands::agent_detail,
            commands::forget_pairing,
            commands::respond,
            commands::send_keys,
            commands::mark_seen,
            commands::set_notifications,
            commands::use_local_host,
            commands::use_remote_host,
            commands::restart_host,
            commands::agent_catalog,
            commands::create_conversation,
            commands::pick_folder,
            commands::stop_agent,
            pairing_commands::pair_link,
            pairing_commands::lan_pairing,
            pairing_commands::set_lan_pairing,
            chat_commands::chat_home,
            chat_commands::chat_search,
            chat_commands::chat_open,
            chat_commands::chat_history,
            chat_commands::chat_refresh,
            chat_commands::chat_approval_decision,
            chat_commands::chat_approval_settings,
            chat_commands::chat_pending_approvals,
            chat_commands::chat_approvals_with_status,
            chat_commands::chat_set_approval_mode,
            chat_commands::chat_send,
            chat_commands::chat_mark_read,
            chat_commands::chat_create,
            chat_commands::chat_update,
            chat_commands::chat_save_section,
            chat_commands::chat_delegation,
            chat_commands::chat_cancel_delegation,
            chat_commands::chat_cancel_conversation
        ])
        .build(tauri::generate_context!())
    {
        Ok(app) => app,
        Err(error) => {
            eprintln!("no se pudo iniciar herdr notify: {error}");
            std::process::exit(1);
        }
    };

    app.run(|handle, event| {
        if matches!(
            event,
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
        ) {
            if let Some(state) = handle.try_state::<Arc<AppState>>() {
                state.stop_local_host();
            }
        }
    });
}
