//! The calendar as a `.ics` file (`/v1/calendar/export.ics`, `/v1/calendar/import`). The
//! file goes out and comes in as it is: the host reads it. Export needs READ_STATE,
//! import RESPOND; both are signed like the rest of the chat routes.

use std::time::Duration;

use reqwest::header::CONTENT_DISPOSITION;
use reqwest::{Method, Response};
use serde::{Deserialize, Serialize};

use super::error::{decode, status_failure, ChatFailure, ChatResult};
use crate::client::HostClient;
use crate::error::CoreError;

/// The largest file the host takes in, in bytes. A bigger one is refused before it is sent.
pub const ICS_MAX_BYTES: usize = 512 * 1024;
const ICS_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_FILE_NAME: &str = "excubia.ics";

/// The host's account of an import. `details` names at most 50 skipped events.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct IcsImport {
    pub imported: u32,
    pub skipped_existing: u32,
    pub skipped_unsupported: u32,
    pub details: Vec<IcsSkip>,
}

/// One skipped event: its place in the file, counted from 0, and the host's reason.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct IcsSkip {
    pub index: u32,
    pub reason: String,
}

/// The file the host sent, with the name a save dialog should propose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcsExport {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

impl HostClient {
    /// The events from `from` up to `to` (not included), or every event without a window.
    pub async fn calendar_export_ics(&self, window: Option<(&str, &str)>) -> ChatResult<IcsExport> {
        let query: Vec<(&str, String)> = window
            .map(|(from, to)| vec![("from", from.to_string()), ("to", to.to_string())])
            .unwrap_or_default();
        let response = self
            .send_ics(Method::GET, "/v1/calendar/export.ics", &query, None, false)
            .await?;

        let status = response.status();
        let disposition = response
            .headers()
            .get(CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let bytes = response
            .bytes()
            .await
            .map_err(|error| ChatFailure::from_core(&CoreError::from(error), false))?;
        if !status.is_success() {
            return Err(status_failure(status, &bytes, None, false));
        }
        Ok(IcsExport {
            file_name: export_file_name(disposition.as_deref()),
            bytes: bytes.to_vec(),
        })
    }

    /// Sends the file as it is. A file over `ICS_MAX_BYTES` is refused without a request.
    pub async fn calendar_import_ics(&self, file: &[u8]) -> ChatResult<IcsImport> {
        if file.len() > ICS_MAX_BYTES {
            return Err(ChatFailure::FileRefused {
                code: "too_large".to_string(),
            });
        }
        let response = self
            .send_ics(
                Method::POST,
                "/v1/calendar/import",
                &[],
                Some((file, "text/calendar")),
                true,
            )
            .await?;
        decode(response, true).await
    }

    /// One signed `.ics` request. The body is the file itself, with its media type.
    async fn send_ics(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<(&[u8], &str)>,
        write: bool,
    ) -> ChatResult<Response> {
        self.signed_typed(method, path, query, body)
            .map_err(|error| ChatFailure::from_core(&error, write))?
            .timeout(ICS_TIMEOUT)
            .send()
            .await
            .map_err(|error| ChatFailure::from_core(&CoreError::from(error), write))
    }
}

/// The name a save dialog proposes: the host's `filename` when it is a plain `.ics` name,
/// else `excubia.ics`. A name with a separator, a `..`, or another extension never gets through.
pub fn export_file_name(disposition: Option<&str>) -> String {
    disposition
        .and_then(filename_parameter)
        .filter(|name| is_plain_ics_name(name))
        .unwrap_or(DEFAULT_FILE_NAME)
        .to_string()
}

fn filename_parameter(disposition: &str) -> Option<&str> {
    disposition
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("filename="))
        .map(|name| name.trim_matches('"'))
}

fn is_plain_ics_name(name: &str) -> bool {
    !name.starts_with('.')
        && !name.contains("..")
        && name.to_ascii_lowercase().ends_with(".ics")
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ' ')
        })
}
