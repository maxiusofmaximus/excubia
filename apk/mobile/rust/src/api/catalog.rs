//! `GET /v1/catalog`: the agent names this host is willing to start.
//!
//! The client only ever sends a name back; commands, arguments and resolved
//! paths never cross this boundary.

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub name: String,
    pub available: bool,
    /// Display hint; falls back to `name`.
    pub label: String,
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_catalog(json: String) -> Option<Vec<CatalogEntry>> {
    let wire: WireCatalog = serde_json::from_str(&json).ok()?;
    if !wire.ok {
        return None;
    }
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
                    name,
                    available: agent.available,
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
    #[serde(default)]
    agents: Vec<WireAgent>,
}

#[derive(Debug, Deserialize)]
struct WireAgent {
    name: Option<String>,
    #[serde(default)]
    available: bool,
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
}
