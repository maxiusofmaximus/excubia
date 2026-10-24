//! The host's agent catalogue: `agents.toml` in the data directory.
//!
//! The file is the closed list `POST /v1/workspaces` resolves against: a
//! client names an entry, the host runs the command the file holds. The
//! default file is seeded from the agents `hn-core-detect` knows.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use hn_core_api::AgentCatalog;
use hn_core_detect::AgentKind;

/// Agents the nucleus's default catalogue offers. The default file also
/// carries a display label per entry.
const DEFAULT_AGENTS: [AgentKind; 4] = [
    AgentKind::Claude,
    AgentKind::Codex,
    AgentKind::OpenCode,
    AgentKind::Gemini,
];

/// Read the catalogue, creating the default file on first run.
pub fn load_or_create(data_dir: &Path) -> Result<AgentCatalog> {
    let path = crate::paths::agents_path(data_dir);
    if !path.is_file() {
        hn_core_persist::write_atomic(&path, default_catalog().as_bytes())
            .with_context(|| format!("write {}", path.display()))?;
    }
    AgentCatalog::open(path).map_err(|error| anyhow!("{error}"))
}

/// The default catalogue text, one entry per known agent with a display label.
pub fn default_catalog() -> String {
    let mut text = String::from(
        "# Agents POST /v1/workspaces may start. The client names one of these\n\
         # entries; it can never name an executable. Edit the commands to match\n\
         # this machine.\n",
    );
    for kind in DEFAULT_AGENTS {
        if let Some(executable) = kind.executable() {
            text.push_str(&format!(
                "\n[agents.{}]\ncommand = \"{executable}\"\nargs = []\nlabel = \"{}\"\n",
                kind.label(),
                display_label(kind),
            ));
        }
    }
    text
}

/// The label the default catalogue gives an agent.
fn display_label(kind: AgentKind) -> &'static str {
    match kind {
        AgentKind::Claude => "Claude Code",
        AgentKind::Codex => "Codex",
        AgentKind::OpenCode => "opencode",
        AgentKind::Gemini => "Gemini CLI",
        AgentKind::Fake => "fake agent",
        AgentKind::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hn_core_test_dir::TestDir;

    #[test]
    fn the_default_catalogue_parses_and_names_the_known_agents() {
        let catalog = AgentCatalog::from_toml(&default_catalog()).expect("default catalogue");
        assert_eq!(
            catalog.names(),
            vec!["claude", "codex", "gemini", "opencode"]
        );
        assert_eq!(
            catalog.resolve("opencode").expect("spec").command,
            "opencode"
        );
    }

    #[test]
    fn load_creates_the_file_once_and_keeps_edits() {
        let dir = TestDir::new("agents-tests");

        let catalog = load_or_create(&dir).expect("first load");
        assert_eq!(catalog.names().len(), 4);

        // An edit wins: the file is the operator's.
        let path = crate::paths::agents_path(&dir);
        std::fs::write(&path, "[agents.mine]\ncommand = \"mine\"\n").expect("write");
        let catalog = load_or_create(&dir).expect("second load");
        assert_eq!(catalog.names(), vec!["mine"]);
    }
}
