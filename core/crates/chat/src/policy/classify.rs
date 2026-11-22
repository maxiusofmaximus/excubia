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

/// The category of an action, or `None` when the payload does not say enough. A `cd` into
/// `project_root` is a read; with no root known it is a command.
pub fn classify(
    kind: ApprovalKind,
    tool: &str,
    payload: Option<&Value>,
    location: Location,
    project_root: Option<&str>,
) -> Option<Category> {
    if location == Location::Outside {
        return Some(Category::OutsideWorkspace);
    }
    match kind {
        ApprovalKind::Command => payload
            .and_then(|value| value["command"].as_str())
            .map(|command| command_category_within(command, project_root)),
        ApprovalKind::Network => Some(Category::Network),
        ApprovalKind::Edit => Some(Category::Edit),
        ApprovalKind::Tool => READ_TOOLS.contains(&tool).then_some(Category::Read),
    }
}

/// The category of a shell command with no project root known: a `cd` is then a command.
#[cfg(test)]
pub fn command_category(command: &str) -> Category {
    command_category_within(command, None)
}

/// The category of a shell command. A chain (`a && b`, `a; b`, `a | b`) is as risky as its
/// riskiest link, so every link is classified; a redirection or a substitution (`>`, `<`,
/// `` ` ``, `$(`) can write or run anything, so the command is then at least a command.
fn command_category_within(command: &str, project_root: Option<&str>) -> Category {
    let (links, opaque) = split_chain(command);
    let (mut tested, mut built, mut plain) = (false, false, false);
    for link in &links {
        match link_category(link, project_root) {
            Category::Read => {}
            Category::Test => tested = true,
            Category::Build => built = true,
            // A plain command does not hide a deletion, a publication or a request: the
            // specific category of any link wins over it.
            Category::Command => plain = true,
            other => return other,
        }
    }
    match (links.is_empty() || opaque || plain, built, tested) {
        (true, _, _) => Category::Command,
        (false, true, _) => Category::Build,
        (false, false, true) => Category::Test,
        (false, false, false) => Category::Read,
    }
}

/// The links of a chain, split on `&`, `;`, `|` and line breaks outside quotes; `None` when
/// and whether the command redirects, substitutes or leaves a quote open (it is opaque).
fn split_chain(command: &str) -> (Vec<&str>, bool) {
    let chars: Vec<(usize, char)> = command.char_indices().collect();
    let mut links = Vec::new();
    let (mut start, mut quote, mut at, mut opaque) = (0, None, 0, false);
    while at < chars.len() {
        let (index, ch) = chars[at];
        let next = chars.get(at + 1).map(|(_, next)| *next);
        match quote {
            Some('\'') if ch == '\'' => quote = None,
            Some('\'') => {}
            Some(_) if ch == '"' => quote = None,
            Some(_) if ch == '\\' => at += 1,
            Some(_) if ch == '`' || (ch == '$' && next == Some('(')) => opaque = true,
            Some(_) => {}
            None if ch == '\'' || ch == '"' => quote = Some(ch),
            None if ch == '\\' => at += 1,
            None if ch == '>' && harmless_redirect(&command[index..]).is_some() => {
                at += harmless_redirect(&command[index..]).unwrap_or(1) - 1;
            }
            None if matches!(ch, '>' | '<' | '`' | '(' | '{')
                || (ch == '$' && next == Some('(')) =>
            {
                opaque = true
            }
            None if matches!(ch, '&' | ';' | '|' | '\n' | '\r') => {
                links.push(&command[start..index]);
                start = index + ch.len_utf8();
            }
            None => {}
        }
        at += 1;
    }
    links.push(&command[start..]);
    links.retain(|link| !link.trim().is_empty());
    (links, opaque || quote.is_some())
}

/// The length of a redirect that writes nothing: `>/dev/null` or `>&1`/`>&2` (as in `2>&1`),
/// `tail` starting at the `>`. Anything else (a file, `>>`) stays opaque.
fn harmless_redirect(tail: &str) -> Option<usize> {
    let rest = tail.strip_prefix('>')?;
    let target = rest.trim_start_matches(' ');
    let skipped = rest.len() - target.len();
    let name = ["/dev/null", "&1", "&2"]
        .into_iter()
        .find(|name| target.starts_with(name))?;
    let after = target[name.len()..].chars().next();
    let ends = after.is_none_or(|ch| ch.is_whitespace() || matches!(ch, ';' | '|' | '&' | ')'));
    ends.then_some(1 + skipped + name.len())
}

/// The category of one command, read from its first words.
fn link_category(command: &str, project_root: Option<&str>) -> Category {
    let mut words: Vec<String> = command
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect();
    let Some(program) = words
        .first()
        .map(|word| program_name(word.as_str()).to_string())
    else {
        return Category::Command;
    };
    let program = program.as_str();
    // A program named by a path is the file at that path, not the tool its name suggests:
    // `./cat.exe` or `tools/rg` can be anything a repository ships. Only an absolute path
    // outside the project (`/usr/bin/cargo`) keeps the name's meaning.
    if words[0].contains(['/', '\\'])
        && !(Path::new(&words[0]).has_root() && !inside_project(&words[0], project_root))
    {
        return Category::Command;
    }
    if matches!(program, "cd" | "set-location" | "sl" | "pushd") {
        return change_dir_category(command, project_root);
    }
    // `git -C <the project> status` is `git status`.
    if program == "git"
        && words.get(1).map(String::as_str) == Some("-c")
        && words.get(2).is_some_and(|dir| inside_project(dir, project_root))
    {
        words.drain(1..3);
    }
    let sub = words.get(1).map(String::as_str);
    let category = match program {
        "git" => git_category(sub, &words),
        "rm" | "del" | "erase" | "rmdir" | "rd" | "ri" | "remove-item" => Category::DeleteUserData,
        "curl" | "wget" | "invoke-webrequest" | "iwr" | "ssh" | "scp" => Category::Network,
        "cat" | "type" | "ls" | "dir" | "head" | "tail" | "rg" | "grep" | "get-content"
        | "findstr" | "wc" | "pwd" | "get-location" | "get-childitem" | "gci" | "echo" => {
            Category::Read
        }
        "cargo" => cargo_category(sub, &words),
        "npm" | "pnpm" | "yarn" => package_category(sub, words.get(2).map(String::as_str)),
        "pytest" => Category::Test,
        "go" if sub == Some("test") && !words.iter().any(|word| runs_a_program(word)) => {
            Category::Test
        }
        _ => Category::Command,
    };
    let reaches_out = words
        .iter()
        .skip(1)
        .any(|word| names_outside(word, project_root) || runs_or_writes(word));
    if matches!(category, Category::Read | Category::Test | Category::Build) && reaches_out {
        Category::Command
    } else {
        category
    }
}

/// `cd <ruta>` only moves, so it is a read when the path is under the project root. No path,
/// a relative one (its place is not known here) or no known root stays a command.
fn change_dir_category(command: &str, project_root: Option<&str>) -> Category {
    let target = command
        .trim()
        .split_once(char::is_whitespace)
        .map(|(_, rest)| rest.trim().trim_matches(['"', '\'']));
    let inside = match (project_root, target) {
        (Some(root), Some(target)) if !target.is_empty() && !target.contains(['"', '\'']) => {
            Path::new(target).has_root() && normalise(target).starts_with(normalise(root))
        }
        _ => false,
    };
    if inside {
        Category::Read
    } else {
        Category::Command
    }
}

/// `git push` publishes; the read-only subcommands only read; the rest changes
/// the repository. `git branch` only reads when it lists (`--show-current`, `-a`, …).
fn git_category(sub: Option<&str>, words: &[String]) -> Category {
    const LISTING: [&str; 8] = [
        "--show-current", "--list", "-a", "--all", "-r", "--remotes", "-v", "-vv",
    ];
    match sub {
        Some("push") => Category::Publish,
        Some(
            "status" | "diff" | "log" | "show" | "rev-parse" | "ls-files" | "blame" | "shortlog"
            | "describe",
        ) => Category::Read,
        Some("branch") if words[2..].iter().all(|word| LISTING.contains(&word.as_str())) => {
            Category::Read
        }
        // `git remote -v` and `git stash list` only list; `remote add`/`stash drop` change.
        Some("remote") if words[2..].iter().all(|w| w == "-v" || w == "--verbose") => {
            Category::Read
        }
        Some("stash") if words.get(2).is_some_and(|w| w == "list") => Category::Read,
        _ => Category::Vcs,
    }
}

/// A flag that makes a read write a file or run a program (`git diff --output=x`,
/// `rg --pre cmd`), so the command is no longer a read.
fn runs_or_writes(word: &str) -> bool {
    // `--hostname-bin` and `--search-zip` make ripgrep start other programs.
    if word == "-z" {
        return true;
    }
    ["--output", "--ext-diff", "--pre", "--hostname-bin", "--search-zip"]
        .iter()
        .any(|flag| word == *flag || word.starts_with(&format!("{flag}=")))
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
fn names_outside(word: &str, project_root: Option<&str>) -> bool {
    if word == "/dev/null" || inside_project(word, project_root) {
        return false;
    }
    // The shell expands `$HOME`, `$env:X` and `%X%` before the program sees them, so what
    // they name is unknown; a git format such as `%h` has no closing `%`.
    let variable = word.contains('$') || has_percent_variable(word);
    let parent_step = word.split(['/', '\\']).any(|part| part == "..");
    let absolute = word
        .split('=')
        .any(|part| part.starts_with(['/', '\\', '~']));
    variable || parent_step || absolute || has_drive_or_url(word)
}

/// `%NAME%` as `cmd` expands it: letters, digits or `_` between two percent signs.
fn has_percent_variable(word: &str) -> bool {
    let parts: Vec<&str> = word.split('%').collect();
    parts.len() >= 3
        && parts[1..parts.len() - 1]
            .iter()
            .any(|part| !part.is_empty() && part.chars().all(|c| c.is_alphanumeric() || c == '_'))
}

/// A colon that points somewhere else: after a lone drive letter (`c:`, `--x=C:/y`) or before `//`
/// (`https://`). A colon inside a label (`"--- status:"`) or a rev path (`HEAD:README.md`) does not.
fn has_drive_or_url(word: &str) -> bool {
    let bytes = word.as_bytes();
    word.match_indices(':').any(|(at, _)| {
        let url = word[at + 1..].starts_with("//");
        let drive = at >= 1
            && bytes[at - 1].is_ascii_alphabetic()
            && (at == 1 || !bytes[at - 2].is_ascii_alphanumeric());
        url || drive
    })
}

/// An absolute path under the project root, as the agent writes it (`C:/…`, or `/c/…` in Git
/// Bash). With no root known, a `..` step, a `=` or a quote left in it, it is not known to be inside.
fn inside_project(word: &str, project_root: Option<&str>) -> bool {
    let Some(root) = project_root else {
        return false;
    };
    let path = word.trim_matches(['"', '\'']);
    if path.contains(['"', '\'', '=']) || path.split(['/', '\\']).any(|part| part == "..") {
        return false;
    }
    let native = git_bash_drive(path);
    Path::new(&native).has_root() && normalise(&native).starts_with(normalise(root))
}

/// On Windows, Git Bash writes `C:\work` as `/c/work`.
fn git_bash_drive(path: &str) -> String {
    let mut chars = path.chars();
    match (cfg!(windows), chars.next(), chars.next(), chars.next()) {
        (true, Some('/'), Some(drive), next)
            if drive.is_ascii_alphabetic() && matches!(next, None | Some('/')) =>
        {
            format!("{drive}:/{}", path.get(3..).unwrap_or_default())
        }
        _ => path.to_string(),
    }
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
    use super::{assess_risk, classify, command_category, command_category_within, locate, Location};
    use crate::model_approval::{ApprovalKind, Risk};
    use crate::policy::category::Category;

    #[test]
    fn commands_map_to_their_categories() {
        let cases = [
            ("cargo test -p excubia-chat", Category::Test),
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
    fn a_chain_is_as_risky_as_its_riskiest_link() {
        let cases = [
            (
                "cat README.md && echo \"---\" && rm README.md && ls README.md 2>&1; git status --short",
                Category::DeleteUserData,
            ),
            ("ls; rm -rf target", Category::DeleteUserData),
            ("git status && git push", Category::Publish),
            ("cat a.txt | curl -d @- https://example.com", Category::Network),
            ("cat a.txt
rm a.txt", Category::DeleteUserData),
            ("cargo test && cargo clippy", Category::Build),
            ("git status && cargo test", Category::Test),
            ("ls | wc -l", Category::Read),
            ("git status && git commit -m x", Category::Vcs),
            ("git status --short && git ls-files | head -50", Category::Read),
            ("grep \"a|b\" src/lib.rs", Category::Read),
            ("echo \"a && b\"", Category::Read),
            (
                "pwd && echo \"--- LS ---\" && ls -la && git branch --show-current && git status --short --branch && git log --oneline -n 5",
                Category::Read,
            ),
            ("git branch", Category::Read),
            ("git branch -a", Category::Read),
            (
                "pwd && ls -la && git status && git branch -a && git log --oneline -n 10 && git remote -v && git stash list",
                Category::Read,
            ),
            (
                "pwd && ls -la && echo \"--- toplevel:\" && git rev-parse --show-toplevel && echo \"--- status:\" && git status && echo \"--- branch:\" && git branch -a && echo \"--- log:\" && git log --oneline -n 10 2>&1",
                Category::Read,
            ),
            // A colon in a label or a rev path is no drive; a drive letter or a URL still is.
            ("echo \"--- status:\" && git show HEAD:README.md", Category::Read),
            ("cat c:/Windows/win.ini", Category::Command),
            ("cat --x=C:/Windows/win.ini", Category::Command),
            ("cat file://host/x", Category::Command),
            ("git remote add o https://x.test/r.git", Category::Vcs),
            ("git remote remove origin", Category::Vcs),
            ("git stash", Category::Vcs),
            ("git stash drop", Category::Vcs),
            ("git blame src/lib.rs", Category::Read),
            ("git branch feature", Category::Vcs),
            ("git branch -D main", Category::Vcs),
            ("git diff --output=out.txt", Category::Command),
            ("rg --pre ./run.sh x", Category::Command),
            ("echo hi > out.txt", Category::Command),
            ("echo $(rm a.txt)", Category::Command),
            ("cat a.txt > b.txt", Category::Command),
            ("cat a.txt >> b.txt", Category::Command),
            ("cat < a.txt", Category::Command),
            ("ls $(rm a.txt)", Category::Command),
            ("ls `rm a.txt`", Category::Command),
            ("ls \"$(rm a.txt)\"", Category::Command),
            ("cat 'unterminated", Category::Command),
            ("git status;", Category::Read),
            ("&&", Category::Command),
            ("ls -la && cat README.md 2>/dev/null | head -50; git status --short", Category::Read),
            (
                "Get-Content -Path README.md -Raw; if ($?) { Remove-Item -Path README.md }",
                Category::Command,
            ),
            ("Get-Content (Remove-Item README.md -PassThru)", Category::Command),
            ("Get-ChildItem | ForEach-Object { Remove-Item $_ }", Category::Command),
            ("(rm a.txt)", Category::Command),
            ("ri a.txt", Category::DeleteUserData),
            ("Get-Content README.md", Category::Read),
            ("pwd && ls -la && git status --short && git ls-files | head -50", Category::Read),
            ("Get-ChildItem -Name; Get-Location", Category::Read),
            ("pwd && rm a.txt", Category::DeleteUserData),
            ("cat a.txt 2>&1", Category::Read),
            ("cat a.txt > /dev/null", Category::Read),
            ("rm a.txt 2>/dev/null", Category::DeleteUserData),
            ("cat a.txt >/dev/nullx", Category::Command),
            ("cat a.txt 2>&1 > out.txt", Category::Command),
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

    // Audit D5: what the shell expands or resolves is not what the words say.
    #[test]
    fn a_variable_can_point_outside_the_project() {
        for command in [
            "cat $HOME/.ssh/id_rsa",
            "echo $GITHUB_TOKEN",
            r"type %USERPROFILE%\.ssh\id_rsa",
            "Get-Content $env:APPDATA/x",
        ] {
            assert_eq!(command_category(command), Category::Command, "{command}");
        }
        // A git format is not a variable.
        assert_eq!(command_category("git log --format=%h -3"), Category::Read);
    }

    #[test]
    fn a_program_named_by_a_path_is_not_the_known_program() {
        for command in ["./cat.exe notes.txt", r".\git.exe status", "tools/rg pattern"] {
            assert_eq!(command_category(command), Category::Command, "{command}");
        }
        assert_eq!(command_category("/usr/bin/cargo test"), Category::Test);
    }

    #[test]
    fn a_search_flag_that_runs_a_program_is_not_a_read() {
        assert_eq!(command_category("rg --hostname-bin x foo"), Category::Command);
        assert_eq!(command_category("rg -z foo"), Category::Command);
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
            classify(ApprovalKind::Edit, "Edit", None, Location::Project, None),
            Some(Category::Edit)
        );
        assert_eq!(
            classify(ApprovalKind::Edit, "Write", None, Location::Outside, None),
            Some(Category::OutsideWorkspace)
        );
        assert_eq!(
            classify(ApprovalKind::Network, "WebFetch", None, Location::Project, None),
            Some(Category::Network)
        );
    }

    #[test]
    fn a_web_search_is_a_network_card_like_a_web_fetch() {
        for location in [Location::Project, Location::Unknown] {
            assert_eq!(
                classify(ApprovalKind::Network, "WebSearch", None, location, None),
                Some(Category::Network),
                "{location:?}"
            );
        }
    }

    #[test]
    fn a_cd_into_the_project_is_a_read_and_elsewhere_a_command() {
        let root = Some("/work/project");
        let cases = [
            ("cd \"/work/project\" && ls -la && git status --short", Category::Read),
            ("cd \"/work/project/src\" && cat a", Category::Read),
            ("cd /work/project && cat a.txt", Category::Read),
            ("cd \"/work/project\" && rm a.txt", Category::DeleteUserData),
            ("cd \"/work/project\" && cat ../x", Category::Command),
            ("cd \"/work/project/../other\" && ls", Category::Command),
            ("cd \"/work/project\" extra && ls", Category::Command),
            ("cd src && ls", Category::Command),
            ("cd .. && ls", Category::Command),
            ("cd /etc && ls", Category::Command),
            ("cd ~ && ls", Category::Command),
            ("cd && ls", Category::Command),
        ];
        for (command, expected) in cases {
            assert_eq!(command_category_within(command, root), expected, "{command}");
        }
        // With no project root known, a cd into it is still a command.
        assert_eq!(
            command_category("cd \"/work/project\" && ls -la && git status --short"),
            Category::Command
        );
    }

    #[test]
    fn an_absolute_path_inside_the_project_is_not_outside() {
        let root = Some("/work/project");
        let cases = [
            ("ls -la /work/project/src", Category::Read),
            ("ls -la /work/project && git -C /work/project status", Category::Read),
            ("cat \"/work/project/a.txt\"", Category::Read),
            ("ls /etc", Category::Command),
            ("ls /work/project/../other", Category::Command),
            ("ls /work/projectile", Category::Command),
            ("git -C /etc status", Category::Vcs),
            ("git -C /work/project push", Category::Publish),
            ("git -C /work/project commit -m x", Category::Vcs),
            ("rm /work/project/a.txt", Category::DeleteUserData),
        ];
        for (command, expected) in cases {
            assert_eq!(command_category_within(command, root), expected, "{command}");
        }
        // With no root known nothing absolute is known to be inside.
        assert_eq!(command_category("ls -la /work/project/src"), Category::Command);
    }

    #[cfg(windows)]
    #[test]
    fn git_bash_writes_a_drive_as_a_letter_folder() {
        let root = Some("c:/work/project");
        assert_eq!(
            command_category_within(
                "ls -la /c/work/project && echo --- && git -C /c/work/project status",
                root
            ),
            Category::Read
        );
        assert_eq!(command_category_within("ls /c/other", root), Category::Command);
        assert_eq!(command_category_within("ls /d/work/project", root), Category::Command);
    }

    #[test]
    fn a_cd_into_the_project_is_low_risk_only_with_a_known_root() {
        let payload = serde_json::json!({
            "command": "cd \"/work/project\" && ls -la && git status --short"
        });
        let inside = classify(
            ApprovalKind::Command,
            "Bash",
            Some(&payload),
            Location::Project,
            Some("/work/project"),
        );
        assert_eq!(inside, Some(Category::Read));
        assert_eq!(assess_risk(inside, Location::Project), Risk::Low);
        let unknown = classify(
            ApprovalKind::Command,
            "Bash",
            Some(&payload),
            Location::Project,
            None,
        );
        assert_eq!(unknown, Some(Category::Command));
        assert_eq!(assess_risk(unknown, Location::Project), Risk::High);
    }
}
