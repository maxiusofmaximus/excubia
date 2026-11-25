//! The two Windows x64 distributions the installer may fetch. They are pinned in
//! code: the URL, the size and the SHA256 are the ones the upstream release
//! publishes (checked 2026-11-25 against the release digests). Nothing from the
//! model, the card or the network chooses them.

/// The tools this profile can install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Git,
    GitHubCli,
}

/// One pinned distribution. Owned strings so tests can build a fixture with the
/// same shape; production code only uses [`pinned`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Distribution {
    pub tool: Tool,
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    /// Folder under `<data>/toolchain` the archive is extracted into.
    pub dir: String,
    /// The executable, relative to `dir`, with `/` separators.
    pub exe: String,
    /// Text the `--version` output must contain.
    pub probe_marker: String,
}

impl Distribution {
    pub fn git() -> Self {
        Self {
            tool: Tool::Git,
            version: "2.56.1".into(),
            url: "https://github.com/git-for-windows/git/releases/download/v2.56.1.windows.1/MinGit-2.56.1-64-bit.zip".into(),
            sha256: "4a92850b6e917da4fd010669b614b55dccf5b0ec333a534cc4be4700dfefdafd".into(),
            size: 39_849_768,
            dir: "git".into(),
            exe: "cmd/git.exe".into(),
            probe_marker: "git version 2.56.1".into(),
        }
    }

    pub fn gh() -> Self {
        Self {
            tool: Tool::GitHubCli,
            version: "2.105.0".into(),
            url: "https://github.com/cli/cli/releases/download/v2.105.0/gh_2.105.0_windows_amd64.zip".into(),
            sha256: "8813a72c899900afaa10e3fec8fbc64b908c89dbfe837e95ed2fa500fc140627".into(),
            size: 15_212_614,
            dir: "gh".into(),
            exe: "bin/gh.exe".into(),
            probe_marker: "gh version 2.105.0".into(),
        }
    }

    /// The name the card and the logs use.
    pub fn label(&self) -> &'static str {
        match self.tool {
            Tool::Git => "Git",
            Tool::GitHubCli => "GitHub CLI",
        }
    }
}

/// Both pinned distributions, Git first.
pub fn pinned() -> [Distribution; 2] {
    [Distribution::git(), Distribution::gh()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pinned_source_is_https_on_github_with_a_full_digest() {
        for dist in pinned() {
            assert!(dist.url.starts_with("https://github.com/"), "{}", dist.url);
            assert_eq!(dist.sha256.len(), 64);
            assert!(dist.sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
            assert!(dist.size > 1_000_000);
        }
    }

    #[test]
    fn the_executables_are_relative_and_inside_their_folder() {
        for dist in pinned() {
            assert!(!dist.exe.starts_with('/') && !dist.exe.contains(".."), "{}", dist.exe);
            assert!(!dist.dir.contains('/') && !dist.dir.contains('\\'));
        }
        assert_eq!(Distribution::git().exe, "cmd/git.exe");
        assert_eq!(Distribution::gh().exe, "bin/gh.exe");
    }
}
