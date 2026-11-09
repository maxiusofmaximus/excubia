//! The `.ics` file of «Calendario». The dialogs and the file itself are the shell's: the
//! webview gets the answer and the name it may show, never a path it did not pick.

use std::io::Read;
use std::path::Path;

use herdr_desktop_core::chat::{ChatFailure, IcsImport, ICS_MAX_BYTES};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::chat_commands::{client, shared, Shared};
use crate::file_dialog::chosen;

#[tauri::command]
pub async fn calendar_import_ics(
    app: AppHandle,
    state: State<'_, Shared>,
) -> Result<Option<IcsImport>, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let Some(path) = chosen(|done| {
        app.dialog()
            .file()
            .add_filter("Calendario", &["ics"])
            .pick_file(done)
    })
    .await
    else {
        return Ok(None);
    };
    let file = read_capped(&path)?;
    client.calendar_import_ics(&file).await.map(Some)
}

/// `from` and `to` name the window; without both, every event is exported. Returns the
/// name written, or `None` when the save dialog was closed.
#[tauri::command]
pub async fn calendar_export_ics(
    app: AppHandle,
    state: State<'_, Shared>,
    from: Option<String>,
    to: Option<String>,
) -> Result<Option<String>, ChatFailure> {
    let shared = shared(&state);
    let export = client(&shared)?
        .calendar_export_ics(from.as_deref().zip(to.as_deref()))
        .await?;
    let Some(path) = chosen(|done| {
        app.dialog()
            .file()
            .add_filter("Calendario", &["ics"])
            .set_file_name(&export.file_name)
            .save_file(done)
    })
    .await
    else {
        return Ok(None);
    };
    std::fs::write(&path, &export.bytes).map_err(|_| file_refused("write_failed"))?;
    Ok(Some(export.file_name))
}

/// The file's bytes, read at most one byte past the limit: the core refuses anything over it.
fn read_capped(path: &Path) -> Result<Vec<u8>, ChatFailure> {
    let file = std::fs::File::open(path).map_err(|_| file_refused("read_failed"))?;
    let mut bytes = Vec::new();
    file.take(ICS_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| file_refused("read_failed"))?;
    Ok(bytes)
}

fn file_refused(code: &str) -> ChatFailure {
    ChatFailure::FileRefused {
        code: code.to_string(),
    }
}
