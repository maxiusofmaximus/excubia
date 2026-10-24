//! Where an action happens, what category it falls into and how risky it is.
//! Everything here reads the structured payload: the command word, the cwd,
//! the paths. What the agent says about its action never counts.

use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use super::category::Category;
use crate::model_approval::{ApprovalKind, Risk};

/// Tools that only read; the policy may allow them inside the project.
const READ_TOOLS: [&str; 5] = ["read", "grep", "glob", "ls", "view"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    /// The action and every path it names are under the project root.
    Project,
    /// Something named by the action is outside the project root.
    Outside,
    /// The project is not known, or the action names no working directory:
    /// the policy never allows what it cannot place.
    Unknown,
}

pub fn locate(project_root: Option<&str>, cwd: Option<&str>, paths: &[String]) -> Location {
    let Some(root) = project_root.map(normalise) else {
        return Location::Unknown;
    };
    let Some(cwd) = cwd.filter(|cwd| !cwd.is_empty()) else {
        return Location::Unknown;
    };
    let inside = |raw: &str| normalise(raw).starts_with(&root);
    if inside(cwd) && paths.iter().all(|path| inside(path)) {
        Location::Project
    } else {
        Location::Outside
    }
}

/// Whether the working directory sits under the operator's workspaces root.
pub fn in_workspace(workspace: Option<&Path>, cwd: Option<&str>) -> bool {
    match (workspace, cwd.filter(|cwd| !cwd.is_empty())) {
        (Some(root), Some(cwd)) => normalise(cwd).starts_with(normalise(root)),
        _ => false,
    }
}

/// The real path when the file exists (symlinks resolved), otherwise the same
/// path with `..` folded, so a missing file cannot climb out of the project.
fn normalise(raw: impl AsRef<Path>) -> PathBuf {
    let path = raw.as_ref();
    if let Ok(real) = std::fs::canonicalize(path) {
        return real;
    }
    let mut folded = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                folded.pop();
            }
            Component::CurDir => {}
            other => folded.push(other.as_os_str()),
        }
    }
    folded
}

/// The category of an action, or `None` when the payload does not say enough.
pub fn classify(
    kind: ApprovalKind,
    tool: &str,
    payload: Option<&Value>,
    location: Location,
) -> Option<Category> {
    if location == Location::Outside {
        return Some(Category::OutsideWorkspace);
    }
    match kind {
        ApprovalKind::Command => payload
            .and_then(|value| value["command"].as_str())
            .map(command_category),
        ApprovalKind::Network => Some(Category::Network),
        ApprovalKind::Edit => Some(Category::Edit),
        ApprovalKind::Tool => READ_TOOLS.contains(&tool).then_some(Category::Read),
    }
}

/// The category of a shell command, read from its first words.
pub fn command_category(command: &str) -> Category {
    let words: Vec<String> = command
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect();
    let Some(program) = words.first().map(|word| program_name(word.as_str())) else {
        return Category::Command;
    };
    let sub = words.get(1).map(String::as_str);
    let category = match program {
        "git" => git_category(sub),
        "rm" | "del" | "rmdir" | "rd" | "remove-item" => Category::DeleteUserData,
        "curl" | "wget" | "invoke-webrequest" | "iwr" | "ssh" | "scp" => Category::Network,
        "cat" | "type" | "ls" | "dir" | "head" | "tail" | "rg" | "grep" | "get-content"
        | "findstr" | "wc" => Category::Read,
        "cargo" => cargo_category(sub, &words),
        "npm" | "pnpm" | "yarn" => package_category(sub, words.get(2).map(String::as_str)),
        "pytest" => Category::Test,
        "go" if sub == Some("test") && !words.iter().any(|word| runs_a_program(word)) => {
            Category::Test
        }
        _ => Category::Command,
    };
    let reaches_out = words.iter().skip(1).any(|word| names_outside(word));
    if matches!(category, Category::Read | Category::Test | Category::Build) && reaches_out {
        Category::Command
    } else {
        category
    }
}

/// `git push` publishes; the read-only subcommands only read; the rest changes
/// the repository.
fn git_category(sub: Option<&str>) -> Category {
    match sub {
        Some("push") => Category::Publish,
        Some("status" | "diff" | "log" | "show" | "rev-parse") => Category::Read,
        _ => Category::Vcs,
    }
}

/// `cargo fmt --check` only reads; a plain `cargo fmt` rewrites the sources.
fn cargo_category(sub: Option<&str>, words: &[String]) -> Category {
    match sub {
        Some("test") => Category::Test,
        Some("build" | "check" | "clippy") => Category::Build,
        Some("fmt") if words.iter().any(|word| word == "--check") => Category::Build,
        Some("publish") => Category::Publish,
        _ => Category::Command,
    }
}

/// `go test -exec` runs the program it names, so it is not a test run.
fn runs_a_program(word: &str) -> bool {
    word.starts_with("-exec") || word.starts_with("-toolexec")
}

fn package_category(sub: Option<&str>, script: Option<&str>) -> Category {
    match (sub, script) {
        (Some("test"), _) => Category::Test,
        (Some("publish"), _) => Category::Publish,
        (Some("build"), _) | (Some("run"), Some("build")) => Category::Build,
        (Some("run"), Some("test")) => Category::Test,
        _ => Category::Command,
    }
}

/// `cargo.exe` and `/usr/bin/cargo` both name `cargo`.
fn program_name(word: &str) -> &str {
    word.rsplit(['/', '\\'])
        .next()
        .unwrap_or(word)
        .trim_end_matches(".exe")
}

/// A word that could point outside the working directory: a parent step (a
/// `..` path part, so `./...` is not one), an absolute path (also after an `=`,
/// as in `--output=/etc/hosts`), a home reference or a drive or URL colon.
fn names_outside(word: &str) -> bool {
    let parent_step = word.split(['/', '\\']).any(|part| part == "..");
    let absolute = word
        .split('=')
        .any(|part| part.starts_with(['/', '\\', '~']));
    parent_step || absolute || word.contains(':')
}

/// The risk the host gives an action. Only what stays inside the project and
/// reads, tests, builds or edits can be low or medium; the rest is high.
pub fn assess_risk(category: Option<Category>, location: Location) -> Risk {
    match (category, location) {
        (Some(Category::Read | Category::Test | Category::Build), Location::Project) => Risk::Low,
        (Some(Category::Edit), Location::Project) => Risk::Medium,
        _ => Risk::High,
    }
}

#[cfg(test)]
mod tests {
    use super::{classify, command_category, locate, Location};
    use crate::model_approval::ApprovalKind;
    use crate::policy::category::Category;

    #[test]
    fn commands_map_to_their_categories() {
        let cases = [
            ("cargo test -p hn-core-chat", Category::Test),
            ("cargo clippy --workspace", Category::Build),
            ("npm test", Category::Test),
            ("pnpm run build", Category::Build),
            ("git status", Category::Read),
            ("git diff --stat", Category::Read),
            ("git log --oneline -5", Category::Read),
            ("git commit -m fix", Category::Vcs),
            ("git push origin main", Category::Publish),
            ("cargo publish --dry-run", Category::Publish),
            ("cargo fmt --check", Category::Build),
            ("cargo fmt", Category::Command),
            ("npm publish", Category::Publish),
            ("pnpm publish --access public", Category::Publish),
            ("pytest -q", Category::Test),
            ("go test ./...", Category::Test),
            ("wc -l src/main.rs", Category::Read),
            ("rm -rf target", Category::DeleteUserData),
            ("curl https://example.com", Category::Network),
            ("cat src/lib.rs", Category::Read),
            ("npm install left-pad", Category::Command),
            ("", Category::Command),
        ];
        for (command, expected) in cases {
            assert_eq!(command_category(command), expected, "{command}");
        }
    }

    #[test]
    fn a_read_or_test_that_names_an_outside_path_is_only_a_command() {
        assert_eq!(command_category("cat ../secrets"), Category::Command);
        assert_eq!(
            command_category("cargo test --manifest-path /other/Cargo.toml"),
            Category::Command
        );
        assert_eq!(command_category("cargo test"), Category::Test);
        assert_eq!(command_category("wc -l /etc/passwd"), Category::Command);
        assert_eq!(
            command_category("git diff --output=/etc/hosts"),
            Category::Command
        );
    }

    #[test]
    fn a_test_runner_that_executes_a_program_is_only_a_command() {
        assert_eq!(
            command_category("go test -exec sh ./..."),
            Category::Command
        );
        assert_eq!(
            command_category("go test -toolexec tool ./..."),
            Category::Command
        );
    }

    #[test]
    fn an_unknown_project_or_cwd_is_never_placed_inside() {
        assert_eq!(locate(None, Some("/work"), &[]), Location::Unknown);
        assert_eq!(locate(Some("/work"), None, &[]), Location::Unknown);
        assert_eq!(locate(Some("/work"), Some(""), &[]), Location::Unknown);
    }

    #[test]
    fn a_dotdot_cannot_climb_out_of_the_project() {
        let root = "/work/project";
        assert_eq!(
            locate(Some(root), Some("/work/project/src"), &[]),
            Location::Project
        );
        assert_eq!(
            locate(Some(root), Some("/work/project/../other"), &[]),
            Location::Outside
        );
        assert_eq!(
            locate(
                Some(root),
                Some("/work/project"),
                &["/etc/passwd".to_string()]
            ),
            Location::Outside
        );
    }

    #[test]
    fn an_edit_is_its_own_category_and_leaving_the_project_is_outside() {
        assert_eq!(
            classify(ApprovalKind::Edit, "Edit", None, Location::Project),
            Some(Category::Edit)
        );
        assert_eq!(
            classify(ApprovalKind::Edit, "Write", None, Location::Outside),
            Some(Category::OutsideWorkspace)
        );
        assert_eq!(
            classify(ApprovalKind::Network, "WebFetch", None, Location::Project),
            Some(Category::Network)
        );
    }
}
