//! `GET …/attachments/{att}/preview?page=N`: the PNG of one page of a document,
//! rendered once by the host and kept beside the file (DOC5B). Only Excel, Word and
//! PDF documents have one. The answer says how many pages the document has, when the
//! renderer knows it (`X-Preview-Pages`, DOC8).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use excubia_core_chat::{AttachmentError, AttachmentFiles, AttachmentState};
use serde::Deserialize;

use crate::doc_pages::{PageError, PageSource, RenderedPage};
use crate::error::ApiError;
use crate::ApiContext;

use super::{file_store, map_error, owned_attachment};

/// The total number of pages of the document, when it is known.
const PREVIEW_PAGES: &str = "x-preview-pages";

#[derive(Deserialize)]
pub struct PreviewQuery {
    page: Option<String>,
}

/// `GET …/attachments/{att}/preview`: the page asked for, `1` by default.
pub async fn attachment_preview(
    State(ctx): State<ApiContext>,
    Path((conversation_id, attachment_id)): Path<(String, String)>,
    Query(query): Query<PreviewQuery>,
) -> Response {
    match preview(
        &ctx,
        &conversation_id,
        &attachment_id,
        query.page.as_deref(),
    )
    .await
    {
        Ok(rendered) => png_response(rendered),
        Err(error) => error.into_response(),
    }
}

async fn preview(
    ctx: &ApiContext,
    conversation_id: &str,
    attachment_id: &str,
    page: Option<&str>,
) -> Result<RenderedPage, ApiError> {
    let page = page_number(page)?;
    let attachment = owned_attachment(ctx, conversation_id, attachment_id)?;
    if !attachment.previewable() {
        return Err(no_preview());
    }
    let mime = match (attachment.state, attachment.mime.clone()) {
        (AttachmentState::Ready, Some(mime)) => mime,
        _ => return Err(map_error(AttachmentError::NotReady)),
    };
    let files = file_store(ctx)?;
    if let Some(cached) = cached_page(&files, conversation_id, attachment_id, page)? {
        return Ok(cached);
    }
    // Two requests for one page wait here; the second finds the first one's PNG.
    let _rendering = ctx.preview_locks.lock(attachment_id).await;
    if let Some(cached) = cached_page(&files, conversation_id, attachment_id, page)? {
        return Ok(cached);
    }
    let source = PageSource {
        path: files
            .file_path(conversation_id, attachment_id)
            .map_err(map_error)?,
        name: attachment.name,
        mime,
    };
    let docs = Arc::clone(&ctx.docs);
    let rendered = tokio::task::spawn_blocking(move || docs.render_page(&source, page))
        .await
        .map_err(ApiError::internal)?
        .map_err(page_error)?;
    files
        .write_preview(conversation_id, attachment_id, page, &rendered.png)
        .map_err(map_error)?;
    if let Some(pages) = rendered.pages {
        files
            .write_pages(conversation_id, attachment_id, pages)
            .map_err(map_error)?;
    }
    Ok(rendered)
}

/// A page rendered before, with the page count kept beside it.
fn cached_page(
    files: &AttachmentFiles,
    conversation_id: &str,
    attachment_id: &str,
    page: u32,
) -> Result<Option<RenderedPage>, ApiError> {
    let Some(png) = files
        .read_preview(conversation_id, attachment_id, page)
        .map_err(map_error)?
    else {
        return Ok(None);
    };
    let pages = files
        .read_pages(conversation_id, attachment_id)
        .map_err(map_error)?;
    Ok(Some(RenderedPage { png, pages }))
}

fn page_number(page: Option<&str>) -> Result<u32, ApiError> {
    match page {
        None => Ok(1),
        Some(text) => match text.parse::<u32>() {
            Ok(number) if number >= 1 => Ok(number),
            _ => Err(ApiError::coded(
                StatusCode::BAD_REQUEST,
                "invalid_page",
                "page must be a whole number from 1",
            )),
        },
    }
}

fn no_preview() -> ApiError {
    ApiError::coded(
        StatusCode::NOT_FOUND,
        "preview_unavailable",
        "this file has no preview",
    )
}

fn page_error(error: PageError) -> ApiError {
    match error {
        PageError::ToolMissing(message) => {
            ApiError::coded(StatusCode::SERVICE_UNAVAILABLE, "tool_missing", message)
        }
        PageError::PageOutOfRange => ApiError::coded(
            StatusCode::NOT_FOUND,
            "page_out_of_range",
            "the document has no such page",
        ),
        PageError::Unsupported => no_preview(),
        PageError::Failed => ApiError::coded(
            StatusCode::BAD_GATEWAY,
            "preview_failed",
            "the page could not be rendered",
        ),
    }
}

fn png_response(rendered: RenderedPage) -> Response {
    let mut response = (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CACHE_CONTROL, "private, no-store"),
            (
                header::CONTENT_DISPOSITION,
                "inline; filename=\"preview.png\"",
            ),
        ],
        rendered.png,
    )
        .into_response();
    if let Some(pages) = rendered.pages {
        // A number is always a valid header value, so the header is always set.
        if let Ok(value) = HeaderValue::from_str(&pages.to_string()) {
            response.headers_mut().insert(PREVIEW_PAGES, value);
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::page_number;

    #[test]
    fn the_first_page_is_the_default_and_zero_is_refused() {
        assert_eq!(page_number(None).ok(), Some(1));
        assert_eq!(page_number(Some("3")).ok(), Some(3));
        assert!(page_number(Some("0")).is_err());
        assert!(page_number(Some("-1")).is_err());
        assert!(page_number(Some("uno")).is_err());
    }
}
