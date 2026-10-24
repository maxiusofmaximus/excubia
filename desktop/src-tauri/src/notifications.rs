use herdr_desktop_core::state::AgentStatus;
use herdr_desktop_core::AgentAlert;
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
    match alerts {
        [alert] => {
            let title = match alert.status {
                AgentStatus::Blocked => format!("Necesita tu atención: {}", alert.project),
                _ => format!("Terminó: {}", alert.project),
            };
            (title, alert.title.clone(), Some(alert.pane_id.as_str()))
        }
        many => {
            let blocked = many
                .iter()
                .filter(|alert| alert.status == AgentStatus::Blocked)
                .count();
            let title = if blocked == many.len() {
                format!("{} agentes necesitan tu atención", many.len())
            } else if blocked == 0 {
                format!("{} agentes terminaron", many.len())
            } else {
                format!("{} agentes cambiaron de estado", many.len())
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
            project: "herdr-notify".to_string(),
            title: "Tarea".to_string(),
            status,
        }
    }

    #[test]
    fn one_blocked_alert_names_its_pane() {
        let alerts = [alert("w1:p1", AgentStatus::Blocked)];
        let (title, body, pane_id) = compose(&alerts);
        assert_eq!(title, "Necesita tu atención: herdr-notify");
        assert_eq!(body, "Tarea");
        assert_eq!(pane_id, Some("w1:p1"));
    }

    #[test]
    fn one_done_alert_reads_as_finished() {
        let alerts = [alert("w1:p1", AgentStatus::Done)];
        let (title, _, pane_id) = compose(&alerts);
        assert_eq!(title, "Terminó: herdr-notify");
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
        assert_eq!(body, "herdr-notify: Tarea\nherdr-notify: Tarea");
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
}
