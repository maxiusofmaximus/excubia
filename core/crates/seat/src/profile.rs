//! The declarative CLI capability matrix. Selection and launch are data-driven:
//! each shipped profile declares how it receives the role prompt, declares MCP
//! and isolates itself, and `launch` consults that declaration instead of
//! scattering `if cli == "claude"` checks through the host.
//!
//! Evidence: `docs/core/evidence/cli-capabilities.md` (Hito 0). Connecting a
//! real CLI (MCP handshake, prompt delivery) is `[NV]` until a manual turn.

use std::fmt;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliId {
    Claude,
    Codex,
    OpenCode,
    /// Test-only simulated CLI; never produced by [`shipped`].
    Fake,
}

impl CliId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
            Self::Fake => "fake",
        }
    }
}

/// How the role prompt reaches the CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptDelivery {
    /// Claude's `--append-system-prompt-file`.
    SystemPromptFile,
    /// Codex's `developer_instructions` in its profile.
    DeveloperInstructions,
    /// OpenCode's `agent.alfred.prompt`.
    AgentConfig,
    /// Test CLI reads the turn prompt from stdin and ignores the role.
    TestStdin,
}

/// Where the per-process MCP declaration lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpDeclaration {
    ClaudeJson,
    CodexToml,
    OpenCodeJson,
    TestJson,
}

/// What the adapter isolates from the operator's global CLI configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Isolation {
    /// `--settings` + `--setting-sources project`; user settings are not read.
    ClaudeSettings,
    /// `CODEX_HOME` points at the run directory (loses user auth, documented).
    CodexHome,
    /// `OPENCODE_CONFIG_DIR` + project config disabled.
    OpenCodeConfigDir,
    /// Test adapter uses its own run directory.
    TestDir,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookSupport {
    /// A blocking pre-tool hook exists (CORE8 will install it).
    Blocking,
    /// No blocking hook verified; UAC cannot rely on this CLI.
    None,
}

/// One process per turn (prompt on stdin) or a CLI session reused across turns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnMode {
    PtyStdin,
    Session,
}

#[derive(Clone, Debug)]
pub struct CliProfile {
    pub id: CliId,
    pub command: String,
    pub min_version: Version,
    pub prompt: PromptDelivery,
    pub mcp: McpDeclaration,
    pub isolation: Isolation,
    pub hook: HookSupport,
    pub mode: TurnMode,
}

#[derive(Clone, Debug)]
pub struct SelectedProfile {
    pub profile: CliProfile,
    pub version: Version,
}

impl CliProfile {
    /// The fields are data, but not independent: a prompt format only makes
    /// sense with the MCP declaration and isolation it was verified against.
    /// Keep that invariant here instead of discovering it in `launch`.
    pub fn is_consistent(&self) -> bool {
        matches!(
            (self.prompt, self.mcp, self.isolation, self.mode),
            (
                PromptDelivery::SystemPromptFile,
                McpDeclaration::ClaudeJson,
                Isolation::ClaudeSettings,
                TurnMode::PtyStdin
            ) | (
                PromptDelivery::DeveloperInstructions,
                McpDeclaration::CodexToml,
                Isolation::CodexHome,
                TurnMode::PtyStdin
            ) | (
                PromptDelivery::AgentConfig,
                McpDeclaration::OpenCodeJson,
                Isolation::OpenCodeConfigDir,
                TurnMode::Session
            ) | (
                PromptDelivery::TestStdin,
                McpDeclaration::TestJson,
                Isolation::TestDir,
                TurnMode::PtyStdin
            )
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// First numeric `major.minor[.patch]` token in a `--version` output.
    /// Prefixes such as `v` or `codex-cli` are ignored.
    pub fn parse(output: &str) -> Option<Self> {
        output.split_whitespace().find_map(|token| {
            let token = token.trim_start_matches(['v', 'V']);
            let token = token
                .trim_matches(|character: char| !character.is_ascii_digit() && character != '.');
            let mut parts = token.split('.');
            let major = parts.next()?.parse::<u64>().ok()?;
            let minor = parts.next()?.parse::<u64>().ok()?;
            if minor > 999 {
                return None;
            }
            let patch = parts
                .next()
                .and_then(|part| part.parse::<u64>().ok())
                .unwrap_or(0);
            Some(Self::new(major, minor, patch))
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// The profiles the host offers, in preference order. The first installed one
/// with a supported version becomes Alfred's seat.
pub fn shipped() -> Vec<CliProfile> {
    vec![
        CliProfile {
            id: CliId::Claude,
            command: "claude".to_string(),
            min_version: Version::new(2, 1, 0),
            prompt: PromptDelivery::SystemPromptFile,
            mcp: McpDeclaration::ClaudeJson,
            isolation: Isolation::ClaudeSettings,
            hook: HookSupport::Blocking,
            mode: TurnMode::PtyStdin,
        },
        CliProfile {
            id: CliId::Codex,
            command: "codex".to_string(),
            min_version: Version::new(0, 155, 0),
            prompt: PromptDelivery::DeveloperInstructions,
            mcp: McpDeclaration::CodexToml,
            isolation: Isolation::CodexHome,
            hook: HookSupport::Blocking,
            mode: TurnMode::PtyStdin,
        },
        CliProfile {
            id: CliId::OpenCode,
            command: "opencode".to_string(),
            min_version: Version::new(2, 0, 0),
            prompt: PromptDelivery::AgentConfig,
            mcp: McpDeclaration::OpenCodeJson,
            isolation: Isolation::OpenCodeConfigDir,
            hook: HookSupport::None,
            mode: TurnMode::Session,
        },
    ]
}

/// A deterministic adapter for tests: it speaks the same MCP stdio protocol
/// through a simulated CLI, without spending model quota.
pub fn test_profile(command: impl Into<String>) -> CliProfile {
    CliProfile {
        id: CliId::Fake,
        command: command.into(),
        min_version: Version::new(0, 0, 1),
        prompt: PromptDelivery::TestStdin,
        mcp: McpDeclaration::TestJson,
        isolation: Isolation::TestDir,
        hook: HookSupport::None,
        mode: TurnMode::PtyStdin,
    }
}

/// Capability probes must not stall host startup on a broken CLI.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Run `<command> --version`. This is a capability probe, not a model turn. A
/// CLI that does not answer within five seconds is treated as unavailable and
/// killed, so a broken wrapper cannot freeze the daemon.
pub fn probe(profile: &CliProfile) -> Option<Version> {
    let mut child = Command::new(&profile.command)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    pipe.read_to_string(&mut stdout).ok()?;
                }
                if let Some(mut pipe) = child.stderr.take() {
                    pipe.read_to_string(&mut stderr).ok()?;
                }
                if !status.success() {
                    return None;
                }
                return Version::parse(&stdout).or_else(|| Version::parse(&stderr));
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Pick the first profile that is installed and new enough. `None` is a valid
/// outcome: the API answers `409 seat_unavailable` and the host stays up.
pub fn select<F>(profiles: &[CliProfile], probe: F) -> Option<SelectedProfile>
where
    F: Fn(&CliProfile) -> Option<Version>,
{
    profiles.iter().find_map(|profile| {
        if !profile.is_consistent() {
            return None;
        }
        let version = probe(profile)?;
        if version < profile.min_version {
            return None;
        }
        Some(SelectedProfile {
            profile: profile.clone(),
            version,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed() -> impl Fn(&CliProfile) -> Option<Version> {
        |profile| match profile.id {
            CliId::Claude => Version::parse("2.1.305 (Claude Code)"),
            CliId::Codex => Version::parse("codex-cli 0.155.1"),
            CliId::OpenCode => Version::parse("2.0.30"),
            CliId::Fake => Version::parse("0.0.1"),
        }
    }

    #[test]
    fn shipped_matrix_keeps_the_verified_capabilities() {
        let profiles = shipped();
        let claude = &profiles[0];
        assert_eq!(claude.prompt, PromptDelivery::SystemPromptFile);
        assert_eq!(claude.mcp, McpDeclaration::ClaudeJson);
        assert_eq!(claude.isolation, Isolation::ClaudeSettings);
        assert_eq!(claude.hook, HookSupport::Blocking);
        assert_eq!(claude.mode, TurnMode::PtyStdin);

        let codex = &profiles[1];
        assert_eq!(codex.prompt, PromptDelivery::DeveloperInstructions);
        assert_eq!(codex.mcp, McpDeclaration::CodexToml);
        assert_eq!(codex.isolation, Isolation::CodexHome);
        assert_eq!(codex.hook, HookSupport::Blocking);

        let opencode = &profiles[2];
        assert_eq!(opencode.prompt, PromptDelivery::AgentConfig);
        assert_eq!(opencode.mcp, McpDeclaration::OpenCodeJson);
        assert_eq!(opencode.isolation, Isolation::OpenCodeConfigDir);
        assert_eq!(opencode.hook, HookSupport::None);
        assert_eq!(opencode.mode, TurnMode::Session);
    }

    #[test]
    fn version_parser_accepts_the_observed_outputs() {
        assert_eq!(
            Version::parse("2.1.305 (Claude Code)"),
            Some(Version::new(2, 1, 305))
        );
        assert_eq!(
            Version::parse("codex-cli 0.155.1"),
            Some(Version::new(0, 155, 1))
        );
        assert_eq!(Version::parse("v2.0.30"), Some(Version::new(2, 0, 30)));
        assert_eq!(Version::parse("no version here"), None);
    }

    #[test]
    fn selection_skips_missing_and_old_clis() {
        let profiles = shipped();
        let selected = select(&profiles, |profile| match profile.id {
            CliId::Claude => None,
            CliId::Codex => Version::parse("0.150.0"),
            CliId::OpenCode => Version::parse("2.0.30"),
            CliId::Fake => None,
        })
        .expect("opencode is available and new enough");
        assert_eq!(selected.profile.id, CliId::OpenCode);

        let none = select(&profiles, fixed());
        assert_eq!(none.expect("claude first").profile.id, CliId::Claude);
        assert!(select(&profiles, |_| None).is_none());
    }

    #[test]
    fn an_unknown_or_old_cli_is_not_selected() {
        assert!(select(&[test_profile("hn-not-installed")], |_| None).is_none());
        let claude_only = &shipped()[..1];
        assert!(select(claude_only, |_| Version::parse("2.0.0")).is_none());
    }

    #[test]
    fn an_inconsistent_matrix_row_is_not_selected() {
        let mut broken = shipped().remove(0);
        broken.mcp = McpDeclaration::CodexToml;
        assert!(!broken.is_consistent());
        assert!(select(&[broken], |_| Version::parse("2.1.305")).is_none());
    }
}
