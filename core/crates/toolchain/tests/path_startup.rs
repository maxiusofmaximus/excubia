//! The PATH is set at startup, before anything is installed, and a CLI started later
//! finds the tool as soon as the extraction lands. One test per binary: it changes the
//! process environment.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use excubia_test_dir::TestDir;
use excubia_toolchain::install::{sha256_hex, ExecProbe, Installer, Outcome};
use excubia_toolchain::{Distribution, Fetch, Layout};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const FAKE_GH: &str = env!("CARGO_BIN_EXE_fake-gh");

struct ByUrl(HashMap<String, PathBuf>);

impl Fetch for ByUrl {
    fn fetch(&self, url: &str, _max: u64, to: &std::path::Path) -> Result<(), String> {
        let from = self.0.get(url).ok_or("unknown url")?;
        fs::copy(from, to).map(|_| ()).map_err(|e| e.to_string())
    }
}

#[test]
fn the_tool_folders_are_on_path_before_the_install_and_resolve_after_it() {
    let dir = TestDir::new("path-startup");
    let data = dir.join("data");
    let layout = Layout::new(&data);
    let bytes = fs::read(FAKE_GH).expect("fake-gh");
    let zip = dir.join("gh.zip");
    let mut writer = ZipWriter::new(File::create(&zip).expect("zip"));
    writer
        .start_file("bin/gh.exe", SimpleFileOptions::default())
        .expect("start");
    writer.write_all(&bytes).expect("write");
    writer.finish().expect("finish");
    let mut gh = Distribution::gh();
    gh.url = "fixture://gh".into();
    gh.size = fs::metadata(&zip).expect("meta").len();
    gh.sha256 = sha256_hex(&zip).expect("digest");

    // Startup: the folders are on PATH although nothing is installed yet.
    let dists = [Distribution::git(), gh.clone()];
    layout.apply_to_process(&dists).expect("apply");
    let path = std::env::var_os("PATH").expect("PATH");
    let entries: Vec<PathBuf> = std::env::split_paths(&path).collect();
    assert_eq!(entries[0], layout.path_dirs(&dists)[0]);
    assert_eq!(entries[1], layout.path_dirs(&dists)[1]);
    assert_eq!(
        std::env::var("GH_CONFIG_DIR").expect("GH_CONFIG_DIR"),
        layout.gh_config_dir().to_string_lossy()
    );
    assert!(layout.gitconfig().is_file(), "the profile Git file exists from startup");

    // Installing later needs no restart: the same PATH finds the new executable.
    let fetch = ByUrl(HashMap::from([("fixture://gh".to_string(), zip)]));
    let installer = Installer {
        layout: &layout,
        fetch: &fetch,
        probe: &ExecProbe,
    };
    assert!(matches!(
        installer.install(&gh).expect("install"),
        Outcome::Installed { .. }
    ));
    let output = Command::new("gh")
        .arg("--version")
        .output()
        .expect("gh resolves through the profile PATH");
    assert!(String::from_utf8_lossy(&output.stdout).contains("gh version 2.105.0"));
}
