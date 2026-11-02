pub mod agenda_commands;
pub mod agents_seed;
pub mod alert_commands;
pub mod app_state;
pub mod calendar_commands;
pub mod calendar_ics_commands;
pub mod chat_commands;
pub mod commands;
pub mod config;
pub mod devices_commands;
pub mod host_process;
pub mod lan;
pub mod mail_commands;
pub mod notifications;
pub mod pairing_commands;
pub mod team_commands;
pub mod voice_commands;

use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use app_state::{lock, AppState};

pub fn run() {
    // WebView2 keeps cookies, storage and the mic permission in its own folder; move it too.
    if let Some(root) = app_state::profile_root() {
        std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", root.join("webview"));
    }
    let mut builder = tauri::Builder::default();
    // A profile-isolated test instance must not hand over to the installed app.
    if app_state::profile_root().is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    }
    let app = match builder
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
            chat_commands::conversations::chat_home,
            chat_commands::conversations::chat_search,
            chat_commands::conversations::chat_filter_conversations,
            chat_commands::conversations::chat_counts,
            chat_commands::conversations::chat_open,
            chat_commands::conversations::chat_history,
            chat_commands::conversations::chat_refresh,
            chat_commands::approvals::chat_approval_decision,
            chat_commands::approvals::chat_approval_settings,
            chat_commands::approvals::chat_pending_approvals,
            chat_commands::approvals::chat_approvals_with_status,
            chat_commands::approvals::chat_approval_find,
            chat_commands::approvals::chat_set_approval_mode,
            chat_commands::writes::chat_send,
            chat_commands::attachments::chat_attachment_create,
            chat_commands::attachments::chat_attachment_chunk,
            chat_commands::attachments::chat_attachment_complete,
            chat_commands::attachments::chat_attachment_delete,
            chat_commands::attachments::chat_attachment_content,
            voice_commands::voice_transcribe,
            chat_commands::writes::chat_mark_read,
            chat_commands::writes::chat_create,
            chat_commands::writes::chat_update,
            chat_commands::writes::chat_save_section,
            chat_commands::delegation::chat_delegation,
            chat_commands::delegation::chat_cancel_delegation,
            chat_commands::delegation::chat_cancel_conversation,
            chat_commands::delegation::chat_stop_conversation,
            team_commands::team_roles,
            team_commands::team_hires,
            team_commands::team_hire,
            team_commands::team_hire_rename,
            team_commands::team_hire_avatar,
            team_commands::team_hire_dismiss,
            team_commands::team_onboarding,
            team_commands::team_onboarding_skip,
            team_commands::team_onboarding_contract,
            agenda_commands::agenda_notes,
            agenda_commands::agenda_note,
            agenda_commands::agenda_note_create,
            agenda_commands::agenda_note_update,
            agenda_commands::agenda_note_delete,
            agenda_commands::agenda_contacts,
            agenda_commands::agenda_contact,
            agenda_commands::agenda_contact_create,
            agenda_commands::agenda_contact_update,
            agenda_commands::agenda_contact_delete,
            calendar_commands::calendar_timezone,
            calendar_commands::calendar_set_timezone,
            calendar_commands::calendar_events,
            calendar_commands::calendar_get,
            calendar_commands::calendar_create,
            calendar_commands::calendar_update,
            calendar_commands::calendar_delete,
            calendar_ics_commands::calendar_import_ics,
            calendar_ics_commands::calendar_export_ics,
            alert_commands::alert_rules,
            alert_commands::alert_rule_create,
            alert_commands::alert_rule_update,
            alert_commands::alert_rule_delete,
            alert_commands::alert_mode,
            alert_commands::alert_mode_set,
            devices_commands::devices_list,
            devices_commands::devices_revoke,
            devices_commands::devices_grant,
            mail_commands::mail_accounts,
            mail_commands::mail_login_start,
            mail_commands::mail_login_status,
            mail_commands::mail_send_enable,
            mail_commands::mail_send_enable_status,
            mail_commands::mail_send_disable,
            mail_commands::mail_draft,
            mail_commands::mail_draft_update,
            mail_commands::mail_draft_send,
            mail_commands::mail_send_state,
            mail_commands::mail_send_cancel
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
