//! Agent identification from a program name.
//!
//! Adapted from herdr (Apache-2.0), `src/detect/mod.rs` (`lookup_agent`,
//! `path_basename`); trimmed to the agents this prototype ships manifests
//! for, plus the fake test agent. See NOTICE.

use std::path::Path;

/// The agent families this core can detect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentKind {
    Claude,
    Codex,
    Gemini,
    OpenCode,
    Fake,
    Unknown,
}

impl AgentKind {
    pub fn label(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
            AgentKind::Gemini => "gemini",
            AgentKind::OpenCode => "opencode",
            AgentKind::Fake => "fake",
            AgentKind::Unknown => "unknown",
        }
    }

    /// The executable an agent is normally installed as. Used to seed the
    /// host's agent catalogue; the catalogue, not this, decides what runs.
    pub fn executable(self) -> Option<&'static str> {
        match self {
            AgentKind::Claude => Some("claude"),
            AgentKind::Codex => Some("codex"),
            AgentKind::Gemini => Some("gemini"),
            AgentKind::OpenCode => Some("opencode"),
            AgentKind::Fake => Some("fake-agent"),
            AgentKind::Unknown => None,
        }
    }

    /// The detection manifest to use for this agent, if one is bundled.
    pub fn manifest_id(self) -> Option<&'static str> {
        match self {
            AgentKind::Claude => Some("claude"),
            AgentKind::Codex => Some("codex"),
            AgentKind::Gemini => None,
            AgentKind::OpenCode => Some("opencode"),
            AgentKind::Fake => Some("fake"),
            AgentKind::Unknown => None,
        }
    }
}

/// Map a program name (or path, or `argv0`) to an agent.
pub fn identify_program(program: &str) -> AgentKind {
    let name = normalize_program_name(program);
    match name.as_str() {
        "claude" | "claude-code" => AgentKind::Claude,
        "codex" => AgentKind::Codex,
        "gemini" => AgentKind::Gemini,
        "opencode" | "opencode2" | "open-code" => AgentKind::OpenCode,
        "fake-agent" | "fake_agent" => AgentKind::Fake,
        _ => AgentKind::Unknown,
    }
}

/// Basename without a Windows launcher extension, lower-cased.
fn normalize_program_name(program: &str) -> String {
    let base = Path::new(program)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.to_string());
    let base = base.to_ascii_lowercase();
    for ext in [".exe", ".cmd", ".bat"] {
        if let Some(stripped) = base.strip_suffix(ext) {
            return stripped.to_string();
        }
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    // Adapted from herdr's `identify_known_agents` test (Apache-2.0),
    // trimmed to the alias table this core ships.
    #[test]
    fn identify_known_agents() {
        assert_eq!(identify_program("claude"), AgentKind::Claude);
        assert_eq!(identify_program("claude-code"), AgentKind::Claude);
        assert_eq!(identify_program("codex"), AgentKind::Codex);
        assert_eq!(identify_program("gemini"), AgentKind::Gemini);
        assert_eq!(identify_program("opencode"), AgentKind::OpenCode);
        assert_eq!(identify_program("opencode.exe"), AgentKind::OpenCode);
        assert_eq!(identify_program("opencode2"), AgentKind::OpenCode);
        assert_eq!(identify_program("fake-agent.exe"), AgentKind::Fake);
        assert_eq!(
            identify_program(r"C:\Users\user\.local\bin\claude.exe"),
            AgentKind::Claude
        );
        assert_eq!(
            identify_program("/home/user/.local/bin/opencode"),
            AgentKind::OpenCode
        );
    }

    #[test]
    fn unknown_programs_stay_unknown() {
        assert_eq!(identify_program("bash"), AgentKind::Unknown);
        assert_eq!(identify_program("pwsh.exe"), AgentKind::Unknown);
        assert_eq!(identify_program("not-claude"), AgentKind::Unknown);
    }
}
