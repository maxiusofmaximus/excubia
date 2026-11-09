//! The host's agent catalogue: `agents.toml` in the data directory.
//!
//! The file is the closed list `POST /v1/workspaces` resolves against: a
//! client names an entry, the host runs the command the file holds. The
//! default file is seeded from the agents `hn-core-detect` knows.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use hn_core_api::{AgentCatalog, EngineListing};
use hn_core_detect::AgentKind;
use hn_core_seat::{probe, shipped, CliId, CliProfile, SelectedProfile};

/// Agents the nucleus's default catalogue offers. The default file also
/// carries a display label per entry.
const DEFAULT_AGENTS: [AgentKind; 5] = [
    AgentKind::Claude,
    AgentKind::Codex,
    AgentKind::OpenCode,
    AgentKind::Gemini,
    AgentKind::CommandCode,
];

/// The first default file, before Command Code. Only a file byte-identical to
/// it is upgraded; an operator's edit is never rewritten.
const V1_DEFAULT_CATALOG: &str =
    "# Agents POST /v1/workspaces may start. The client names one of these\n\
     # entries; it can never name an executable. Edit the commands to match\n\
     # this machine.\n\
     \n[agents.claude]\ncommand = \"claude\"\nargs = []\nlabel = \"Claude Code\"\n\
     \n[agents.codex]\ncommand = \"codex\"\nargs = []\nlabel = \"Codex\"\n\
     \n[agents.opencode]\ncommand = \"opencode\"\nargs = []\nlabel = \"opencode\"\n\
     \n[agents.gemini]\ncommand = \"gemini\"\nargs = []\nlabel = \"Gemini CLI\"\n";

/// Read the catalogue, creating the default file on first run and adding the
/// agents a v1 default file lacks.
pub fn load_or_create(data_dir: &Path) -> Result<AgentCatalog> {
    let path = crate::paths::agents_path(data_dir);
    let untouched_v1 = std::fs::read_to_string(&path).is_ok_and(|text| text == V1_DEFAULT_CATALOG);
    if !path.is_file() || untouched_v1 {
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

/// The shipped CLI profiles, with the OpenCode command the catalogue names. On Windows the PATH
/// holds only a `.CMD` shim that `Command` cannot start, so the operator points `agents.toml` at the `.exe`.
pub fn configured_profiles(catalog: &AgentCatalog) -> Vec<CliProfile> {
    let mut profiles = shipped();
    if let Some(spec) = catalog.resolve("opencode") {
        for profile in profiles
            .iter_mut()
            .filter(|profile| matches!(profile.id, CliId::OpenCode))
        {
            profile.command.clone_from(&spec.command);
        }
    }
    profiles
}

/// The CLI engines `Ready` on this PC: OpenCode, when its gate is proven and it runs at least its
/// minimum version. Read at start-up; each turn probes its engine again.
pub fn ready_engines(profiles: &[CliProfile], gate_proven: bool) -> Vec<SelectedProfile> {
    if !gate_proven {
        return Vec::new();
    }
    let Some(profile) = profiles
        .iter()
        .find(|profile| matches!(profile.id, CliId::OpenCode))
    else {
        return Vec::new();
    };
    match probe(profile) {
        Some(version) if version >= profile.min_version => vec![SelectedProfile {
            profile: profile.clone(),
            version,
        }],
        _ => Vec::new(),
    }
}

/// The engines the API lists and Alfred may choose.
pub fn engine_listing(engines: &[SelectedProfile]) -> Vec<EngineListing> {
    engines
        .iter()
        .map(|engine| EngineListing {
            name: engine.profile.id.as_str().to_owned(),
            version: engine.version.to_string(),
        })
        .collect()
}

/// The label the default catalogue gives an agent.
fn display_label(kind: AgentKind) -> &'static str {
    match kind {
        AgentKind::Claude => "Claude Code",
        AgentKind::Codex => "Codex",
        AgentKind::OpenCode => "opencode",
        AgentKind::Gemini => "Gemini CLI",
        AgentKind::CommandCode => "Command Code",
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
            vec!["claude", "codex", "commandcode", "gemini", "opencode"]
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
        assert_eq!(catalog.names().len(), 5);

        // An edit wins: the file is the operator's.
        let path = crate::paths::agents_path(&dir);
        std::fs::write(&path, "[agents.mine]\ncommand = \"mine\"\n").expect("write");
        let catalog = load_or_create(&dir).expect("second load");
        assert_eq!(catalog.names(), vec!["mine"]);
    }

    #[test]
    fn an_untouched_v1_file_gains_command_code_and_an_edited_one_does_not() {
        let dir = TestDir::new("agents-v1-tests");
        let path = crate::paths::agents_path(&dir);

        std::fs::write(&path, V1_DEFAULT_CATALOG).expect("write v1");
        let catalog = load_or_create(&dir).expect("upgrade");
        assert!(catalog.names().iter().any(|name| name == "commandcode"));

        let edited = format!("{V1_DEFAULT_CATALOG}\n[agents.mine]\ncommand = \"mine\"\n");
        std::fs::write(&path, &edited).expect("write edited v1");
        let catalog = load_or_create(&dir).expect("keep edits");
        assert!(!catalog.names().iter().any(|name| name == "commandcode"));
        assert!(catalog.names().iter().any(|name| name == "mine"));
    }

    #[test]
    fn the_catalogue_command_replaces_the_opencode_profile_command() {
        let catalog = AgentCatalog::from_toml(
            "[agents.opencode]\ncommand = \"C:/tools/opencode.exe\"\nargs = []\nlabel = \"opencode\"\n",
        )
        .expect("catalogue");
        let profiles = configured_profiles(&catalog);
        let opencode = profiles
            .iter()
            .find(|profile| matches!(profile.id, CliId::OpenCode))
            .expect("opencode profile");
        assert_eq!(opencode.command, "C:/tools/opencode.exe");
        let claude = profiles
            .iter()
            .find(|profile| matches!(profile.id, CliId::Claude))
            .expect("claude profile");
        assert_eq!(claude.command, "claude");
    }

    #[test]
    fn no_engine_is_listed_without_a_proven_gate() {
        let profiles = shipped();
        assert!(ready_engines(&profiles, false).is_empty());
        assert!(engine_listing(&[]).is_empty());
    }
}
