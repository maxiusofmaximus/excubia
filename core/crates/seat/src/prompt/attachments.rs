//! The attachment lines of the turn prompt and the folders the CLI may read for it.

use std::path::{Path, PathBuf};

use excubia_chat::{ContextAttachment, ContextPage, FileKind};

/// One line per file, from the host's data only: the id, the kind, the size and
/// the path. The name the user chose stays out of the prompt.
pub(super) fn attachment_lines(files: &[ContextAttachment]) -> String {
    files
        .iter()
        .map(|attached| {
            let file = &attached.file;
            let path = attached.path.display();
            match file.kind {
                FileKind::Document => format!(
                    "[adjunto {} documento {} bytes={} path={path}]\n",
                    file.id, file.mime, file.size
                ),
                FileKind::Image => format!(
                    "[adjunto {} {} {}x{} bytes={} path={path}]\n",
                    file.id, file.mime, file.width, file.height, file.size
                ),
            }
        })
        .collect()
}

/// The folders the CLI may read for this turn: the folder of each attached file.
/// A conversation keeps its files in one folder, so this is one entry at most.
pub fn attachment_dirs(context: &ContextPage) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = context
        .attachments
        .values()
        .flatten()
        .filter_map(|attached| attached.path.parent().map(Path::to_path_buf))
        .collect();
    dirs.sort();
    dirs.dedup();
    dirs
}
