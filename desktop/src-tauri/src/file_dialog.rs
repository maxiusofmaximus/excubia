//! The shell's file dialogs, awaited as a plain future.

use std::path::PathBuf;

use tauri_plugin_dialog::FilePath;
use tokio::sync::oneshot;

/// The path the operator picked, or `None` when the dialog was closed.
pub async fn chosen(
    open: impl FnOnce(Box<dyn FnOnce(Option<FilePath>) + Send>),
) -> Option<PathBuf> {
    let (sender, receiver) = oneshot::channel();
    open(Box::new(move |path| {
        let _ = sender.send(path);
    }));
    receiver
        .await
        .ok()
        .flatten()
        .and_then(|path| path.into_path().ok())
}
