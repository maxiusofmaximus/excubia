//! Extracting a downloaded archive into a staging folder, with the entries checked
//! before anything is written: no escape from the folder, no links, no special
//! files, no duplicates, and a cap on how much it may write.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader};
use std::path::{Component, Path};

use zip::ZipArchive;

/// The most entries an archive may hold. The pinned archives hold 378 and 2.
pub const MAX_ENTRIES: usize = 20_000;
/// The most bytes an archive may expand to.
pub const MAX_EXPANDED_BYTES: u64 = 1 << 30;

const S_IFMT: u32 = 0o170_000;
const S_IFREG: u32 = 0o100_000;
const S_IFDIR: u32 = 0o040_000;

/// Extracts every entry under `dest`, which must not hold anything yet.
pub fn extract(archive_path: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|error| format!("open archive: {error}"))?;
    let mut archive = ZipArchive::new(BufReader::new(file))
        .map_err(|error| format!("not a zip archive: {error}"))?;
    if archive.len() > MAX_ENTRIES {
        return Err(format!("{} entries; the limit is {MAX_ENTRIES}", archive.len()));
    }
    // First pass: every entry is checked before anything is written.
    let mut expanded: u64 = 0;
    let mut seen = std::collections::HashSet::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("entry {index}: {error}"))?;
        if entry.is_symlink() {
            return Err(format!("entry {} is a link", entry.name()));
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| format!("entry {} leaves the folder", entry.name()))?;
        if !relative.components().all(|part| matches!(part, Component::Normal(_))) {
            return Err(format!("entry {} has a rooted path", entry.name()));
        }
        let mode = entry.unix_mode().unwrap_or(S_IFREG) & S_IFMT;
        if !(entry.is_dir() || mode == S_IFDIR || mode == S_IFREG || mode == 0) {
            return Err(format!("entry {} is not a regular file", entry.name()));
        }
        if !entry.is_dir() && mode != S_IFDIR && !seen.insert(relative.clone()) {
            return Err(format!("entry {} appears twice", entry.name()));
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_EXPANDED_BYTES {
            return Err(format!("expands past {MAX_EXPANDED_BYTES} bytes"));
        }
    }

    fs::create_dir_all(dest).map_err(|error| format!("create staging: {error}"))?;
    // Second pass: the entries are known to be inside `dest`.
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("entry {index}: {error}"))?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| format!("entry {} leaves the folder", entry.name()))?;
        let target = dest.join(&relative);
        let is_folder = entry.is_dir() || entry.unix_mode().is_some_and(|m| m & S_IFMT == S_IFDIR);
        if is_folder {
            fs::create_dir_all(&target).map_err(|error| format!("create folder: {error}"))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("create folder: {error}"))?;
        }
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|error| format!("write {}: {error}", entry.name()))?;
        io::copy(&mut entry, &mut out).map_err(|error| format!("write {}: {error}", entry.name()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use excubia_test_dir::TestDir;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn archive_with(dir: &Path, entries: &[(&str, &[u8])]) -> std::path::PathBuf {
        let path = dir.join("fixture.zip");
        let mut writer = ZipWriter::new(File::create(&path).expect("create"));
        for (name, bytes) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .expect("start");
            writer.write_all(bytes).expect("write");
        }
        writer.finish().expect("finish");
        path
    }

    #[test]
    fn a_normal_archive_lands_inside_the_destination() {
        let dir = TestDir::new("archive-ok");
        let zip = archive_with(&dir, &[("cmd/git.exe", b"exe"), ("etc/readme", b"x")]);
        let dest = dir.join("out");
        extract(&zip, &dest).expect("extract");
        assert_eq!(std::fs::read(dest.join("cmd").join("git.exe")).expect("read"), b"exe");
    }

    #[test]
    fn an_entry_that_climbs_out_is_refused_and_nothing_is_written() {
        let dir = TestDir::new("archive-slip");
        let zip = archive_with(&dir, &[("ok.txt", b"a"), ("../escape.txt", b"b")]);
        let dest = dir.join("out");
        let error = extract(&zip, &dest).expect_err("refused");
        assert!(error.contains("leaves the folder"), "{error}");
        assert!(!dir.join("escape.txt").exists());
    }

    #[test]
    fn an_absolute_entry_is_refused() {
        let dir = TestDir::new("archive-abs");
        let zip = archive_with(&dir, &[("/etc/passwd", b"a")]);
        assert!(extract(&zip, &dir.join("out")).is_err());
    }

    #[test]
    fn a_file_that_is_not_a_zip_is_refused() {
        let dir = TestDir::new("archive-bad");
        let path = dir.join("bad.zip");
        std::fs::write(&path, b"not a zip at all").expect("write");
        assert!(extract(&path, &dir.join("out")).is_err());
    }
}
