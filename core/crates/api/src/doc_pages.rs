//! The page previews of the chat's documents (DOC5B). The host renders a page with
//! its document CLIs and hands the PNG back; the API asks for a page, keeps it and
//! makes sure two requests for the same page render it once.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

/// The stored file to render, with the name and mime type the user gave it.
pub struct PageSource {
    pub path: PathBuf,
    pub name: String,
    pub mime: String,
}

/// Why a page was not rendered. Each one has its own answer in `errors.md`.
#[derive(Debug, PartialEq, Eq)]
pub enum PageError {
    /// A CLI the page needs is not on this PC; the text says which, in Spanish.
    ToolMissing(String),
    /// The document has no page with that number.
    PageOutOfRange,
    /// The format has no page renderer.
    Unsupported,
    /// The CLI failed or wrote no image.
    Failed,
}

/// One page as PNG, and the document's page count when the renderer knows it.
#[derive(Debug, PartialEq, Eq)]
pub struct RenderedPage {
    pub png: Vec<u8>,
    pub pages: Option<u32>,
}

/// One document CLI the host found at start, as the app names it.
#[derive(Debug, PartialEq, Eq)]
pub struct DocTool {
    /// The stable identifier the app uses: `gridcraft`, `wordcraft`, `pdfcraft`.
    pub app: &'static str,
    /// The name the user reads.
    pub name: &'static str,
    pub available: bool,
    /// The version these apps were tested with, whether present or not.
    pub tested_version: &'static str,
}

/// The host's document CLIs: rendering a page, and the tools it found at start.
/// Rendering runs processes, so the API calls it off the async runtime.
pub trait DocumentPages: Send + Sync {
    fn render_page(&self, source: &PageSource, page: u32) -> Result<RenderedPage, PageError>;

    /// Every document app, in `App::ALL` order, from the discovery done at start.
    fn tools(&self) -> [DocTool; 3];
}

/// One lock per attachment: while a page of it renders, the same request waits
/// here and then reads the cache.
#[derive(Default)]
pub struct PreviewLocks {
    slots: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl PreviewLocks {
    pub async fn lock(&self, attachment_id: &str) -> OwnedMutexGuard<()> {
        let slot = {
            let mut slots = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
            Arc::clone(slots.entry(attachment_id.to_owned()).or_default())
        };
        slot.lock_owned().await
    }
}
