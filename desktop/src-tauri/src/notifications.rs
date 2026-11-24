use excubia_desktop_core::state::AgentStatus;
use excubia_desktop_core::ui_lang::{self, UiLang};
use excubia_desktop_core::AgentAlert;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

/// Shows one grouped system notification for the alerts of a single state
/// revision. A lone alert carries its pane id, so a click can open that agent.
pub fn show_alerts(app: &AppHandle, alerts: &[AgentAlert]) {
    if alerts.is_empty() {
        return;
    }
    let (title, body, pane_id) = compose(alerts);
    let mut builder = app.notification().builder().title(title.clone()).body(body);
    if let Some(pane_id) = pane_id {
        builder = builder.extra("pane_id", pane_id);
    }
    match builder.show() {
        Ok(()) => eprintln!("aviso mostrado: {title}"),
        Err(error) => eprintln!("no se pudo mostrar el aviso del sistema: {error}"),
    }
}

fn compose(alerts: &[AgentAlert]) -> (String, String, Option<&str>) {
    compose_in(alerts, ui_lang::current())
}

fn compose_in(alerts: &[AgentAlert], lang: UiLang) -> (String, String, Option<&str>) {
    match alerts {
        [alert] => {
            let title = match (alert.status, lang.is_en()) {
                (AgentStatus::Blocked, false) => format!("Necesita tu atención: {}", alert.project),
                (AgentStatus::Blocked, true) => format!("Needs your attention: {}", alert.project),
                (_, false) => format!("Terminó: {}", alert.project),
                (_, true) => format!("Finished: {}", alert.project),
            };
            (title, alert.title.clone(), Some(alert.pane_id.as_str()))
        }
        many => {
            let blocked = many
                .iter()
                .filter(|alert| alert.status == AgentStatus::Blocked)
                .count();
            let count = many.len();
            let title = match (blocked == count, blocked == 0, lang.is_en()) {
                (true, _, false) => format!("{count} agentes necesitan tu atención"),
                (true, _, true) => format!("{count} agents need your attention"),
                (_, true, false) => format!("{count} agentes terminaron"),
                (_, true, true) => format!("{count} agents finished"),
                (_, _, false) => format!("{count} agentes cambiaron de estado"),
                (_, _, true) => format!("{count} agents changed state"),
            };
            let body = many
                .iter()
                .map(|alert| format!("{}: {}", alert.project, alert.title))
                .collect::<Vec<_>>()
                .join("\n");
            (title, body, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert(pane_id: &str, status: AgentStatus) -> AgentAlert {
        AgentAlert {
            pane_id: pane_id.to_string(),
            project: "excubia".to_string(),
            title: "Tarea".to_string(),
            status,
        }
    }

    #[test]
    fn one_blocked_alert_names_its_pane() {
        let alerts = [alert("w1:p1", AgentStatus::Blocked)];
        let (title, body, pane_id) = compose(&alerts);
        assert_eq!(title, "Necesita tu atención: excubia");
        assert_eq!(body, "Tarea");
        assert_eq!(pane_id, Some("w1:p1"));
    }

    #[test]
    fn one_done_alert_reads_as_finished() {
        let alerts = [alert("w1:p1", AgentStatus::Done)];
        let (title, _, pane_id) = compose(&alerts);
        assert_eq!(title, "Terminó: excubia");
        assert_eq!(pane_id, Some("w1:p1"));
    }

    #[test]
    fn several_alerts_are_grouped_without_a_single_pane() {
        let alerts = [
            alert("w1:p1", AgentStatus::Blocked),
            alert("w1:p2", AgentStatus::Blocked),
        ];
        let (title, body, pane_id) = compose(&alerts);
        assert_eq!(title, "2 agentes necesitan tu atención");
        assert_eq!(body, "excubia: Tarea\nexcubia: Tarea");
        assert_eq!(pane_id, None, "a grouped toast cannot open one agent");
    }

    #[test]
    fn a_mixed_batch_does_not_claim_everyone_is_blocked() {
        let alerts = [
            alert("w1:p1", AgentStatus::Blocked),
            alert("w1:p2", AgentStatus::Done),
        ];
        let (title, _, _) = compose(&alerts);
        assert_eq!(title, "2 agentes cambiaron de estado");
    }

    #[test]
    fn english_titles_follow_the_language_they_are_given() {
        let one = [alert("w1:p1", AgentStatus::Blocked)];
        let (title, _, pane_id) = compose_in(&one, UiLang::En);
        assert_eq!(title, "Needs your attention: excubia");
        assert_eq!(pane_id, Some("w1:p1"));

        let grouped = [
            alert("w1:p1", AgentStatus::Done),
            alert("w1:p2", AgentStatus::Done),
        ];
        let (title, _, _) = compose_in(&grouped, UiLang::En);
        assert_eq!(title, "2 agents finished");
    }
}
