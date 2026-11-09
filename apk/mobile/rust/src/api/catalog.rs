//! `GET /v1/catalog`: the agent names this host is willing to start.
//!
//! The client only ever sends a name back; commands, arguments and resolved
//! paths never cross this boundary.

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub name: String,
    pub available: bool,
    /// Windows can start it as the host has it right now.
    pub runnable: bool,
    /// The host can repoint it at its `.CMD` shim (needs `APPROVE`).
    pub fixable: bool,
    /// Why it cannot run, as the host words it.
    pub problem: Option<String>,
    /// Display hint; falls back to `name`.
    pub label: String,
    /// The PC's default agent: new conversations start with it selected.
    pub is_default: bool,
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_catalog(json: String) -> Option<Vec<CatalogEntry>> {
    let wire: WireCatalog = serde_json::from_str(&json).ok()?;
    if !wire.ok {
        return None;
    }
    let default = wire.default.map(|name| name.trim().to_string());
    Some(
        wire.agents
            .into_iter()
            .filter_map(|agent| {
                let name = agent.name?.trim().to_string();
                if name.is_empty() {
                    return None;
                }
                let label = agent
                    .label
                    .map(|label| label.trim().to_string())
                    .filter(|label| !label.is_empty())
                    .unwrap_or_else(|| name.clone());
                Some(CatalogEntry {
                    is_default: default.as_deref() == Some(name.as_str()),
                    name,
                    available: agent.available,
                    // A host that predates `runnable` only reports `available`.
                    runnable: agent.runnable.unwrap_or(agent.available),
                    fixable: agent.fixable,
                    problem: agent
                        .problem
                        .map(|problem| problem.trim().to_string())
                        .filter(|problem| !problem.is_empty()),
                    label,
                })
            })
            .collect(),
    )
}

#[derive(Debug, Deserialize)]
struct WireCatalog {
    #[serde(default)]
    ok: bool,
    /// Absent on a host that predates the default agent.
    default: Option<String>,
    #[serde(default)]
    agents: Vec<WireAgent>,
}

#[derive(Debug, Deserialize)]
struct WireAgent {
    name: Option<String>,
    #[serde(default)]
    available: bool,
    runnable: Option<bool>,
    #[serde(default)]
    fixable: bool,
    problem: Option<String>,
    label: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_availability_and_labels() {
        let entries = parse_catalog(
            r#"{"ok":true,"agents":[
                {"name":"fake","available":true,"label":"Fake agent"},
                {"name":"claude","available":false,"label":"Claude Code"},
                {"name":"opencode","available":true}
            ]}"#
            .to_string(),
        )
        .unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "fake");
        assert!(entries[0].available);
        assert_eq!(entries[0].label, "Fake agent");
        assert!(!entries[1].available);
        assert_eq!(entries[2].label, "opencode");
    }

    #[test]
    fn says_what_windows_can_run_and_what_can_be_fixed() {
        let entries = parse_catalog(
            r#"{"ok":true,"agents":[
                {"name":"codex","available":true,"runnable":false,"fixable":true,"problem":" Windows no puede ejecutarlo. "},
                {"name":"claude","available":false,"runnable":false,"fixable":false,"problem":"No está instalado en este equipo."},
                {"name":"opencode","available":true,"runnable":true,"fixable":false,"problem":""},
                {"name":"old","available":true}
            ]}"#
            .to_string(),
        )
        .unwrap();
        assert!(!entries[0].runnable && entries[0].fixable);
        assert_eq!(entries[0].problem.as_deref(), Some("Windows no puede ejecutarlo."));
        assert!(!entries[1].runnable && entries[1].problem.is_some());
        assert!(entries[2].runnable && entries[2].problem.is_none());
        assert!(entries[3].runnable, "a host without the field is trusted as available");
    }

    #[test]
    fn drops_rows_without_a_name_and_refuses_a_failed_envelope() {
        let entries = parse_catalog(
            r#"{"ok":true,"agents":[{"available":true},{"name":"  "},{"name":"fake"}]}"#
                .to_string(),
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "fake");

        assert!(parse_catalog(r#"{"ok":false,"agents":[]}"#.to_string()).is_none());
        assert!(parse_catalog("not json".to_string()).is_none());
    }

    #[test]
    fn marks_the_pcs_default_agent_and_tolerates_hosts_without_one() {
        let entries = parse_catalog(
            r#"{"ok":true,"default":"opencode","agents":[
                {"name":"claude","available":true},
                {"name":"opencode","available":true}
            ]}"#
            .to_string(),
        )
        .unwrap();
        assert!(!entries[0].is_default);
        assert!(entries[1].is_default);

        let older = parse_catalog(
            r#"{"ok":true,"agents":[{"name":"claude","available":true}]}"#.to_string(),
        )
        .unwrap();
        assert!(!older[0].is_default);
    }
}
