//! `POST /v1/chat/projects` (signed, `ADMIN`): the operator opens a folder of his own
//! as a project, as «Añadir proyecto → carpeta local» does in other apps. Only the
//! device that may administer the PC names a folder: a phone never does. The lead and
//! its specialists then work in that folder; the approval policy measures inside and
//! outside against it.

use std::path::{Component, Path, PathBuf};

use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use excubia_chat::NewProject;
use serde::Deserialize;

use crate::error::ApiError;
use crate::routes::parse_body;
use crate::ApiContext;

use super::support;

const MAX_NAME_CHARS: usize = 80;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewFolderProject {
    name: String,
    root_path: String,
}

pub async fn create(State(ctx): State<ApiContext>, body: Bytes) -> Response {
    let payload: NewFolderProject = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    let name = payload.name.trim();
    if name.is_empty()
        || name.chars().count() > MAX_NAME_CHARS
        || name.chars().any(char::is_control)
    {
        return ApiError::invalid_field("invalid_name", "name", "a name of 1 to 80 characters")
            .into_response();
    }
    let root_path = match project_folder(&payload.root_path) {
        Ok(path) => path,
        Err(message) => {
            return ApiError::invalid_field("invalid_path", "root_path", message).into_response()
        }
    };
    match ctx.chat.create_project(NewProject {
        name: name.to_string(),
        root_path,
    }) {
        Ok(project) => support::map_ok("project", project),
        Err(error) => support::map_error(error).into_response(),
    }
}

/// An existing folder, absolute, local, and not a drive or system folder: an agent
/// that works there may write anywhere under it without asking.
pub(crate) fn project_folder(raw: &str) -> Result<PathBuf, &'static str> {
    let raw = raw.trim();
    let path = Path::new(raw);
    if raw.is_empty() || !path.is_absolute() {
        return Err("the folder must be an absolute path");
    }
    if raw.starts_with(r"\\") && !raw.starts_with(r"\\?\") {
        return Err("network folders are not allowed");
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| "the folder does not exist or cannot be read")?;
    if !canonical.is_dir() {
        return Err("the path is not a folder");
    }
    let parts = canonical
        .components()
        .filter(|part| matches!(part, Component::Normal(_)))
        .count();
    if parts == 0 {
        return Err("a whole drive cannot be a project");
    }
    if system_folders()
        .iter()
        .any(|system| canonical.starts_with(system))
    {
        return Err("a system folder cannot be a project");
    }
    Ok(canonical)
}

/// Where the operating system and installed programs live, as the system names them.
fn system_folders() -> Vec<PathBuf> {
    let named = [
        "SystemRoot",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
    ]
    .iter()
    .filter_map(std::env::var_os);
    let unix = ["/etc", "/usr", "/bin", "/sbin", "/System"]
        .iter()
        .map(Into::into);
    named
        .chain(unix)
        .filter_map(|path: std::ffi::OsString| Path::new(&path).canonicalize().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::project_folder;

    #[test]
    fn only_an_existing_local_folder_below_the_drive_is_a_project() {
        let dir = std::env::temp_dir().join(format!("excubia-project-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        assert!(project_folder(&dir.display().to_string()).is_ok());
        assert!(project_folder("relativo/carpeta").is_err());
        assert!(project_folder(&dir.join("no-existe").display().to_string()).is_err());
        assert!(project_folder(r"\\servidor\compartido").is_err());
        if cfg!(windows) {
            assert!(project_folder(r"C:\").is_err());
            assert!(project_folder(r"C:\Windows").is_err());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
