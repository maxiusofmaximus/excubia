//! The document CLIs of the host (DOC5B): the tools found at start, run through
//! `excubia-crafting`. One `Craft` serves every read by `doc.read` and every page
//! the API previews.

use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use excubia_api::{DocTool, DocumentPages, PageError, PageSource, RenderedPage};
use excubia_crafting::{
    App, CraftError, Document, Driver, Kind, Preview, ProcessRunner, ReadOpts, ReadOut, Runner,
    Tools,
};

/// Where the CLIs write their work files: a folder under the data directory.
const WORK_DIR: &str = "craft";

pub struct Craft {
    tools: Tools,
    runner: Arc<dyn Runner + Send + Sync>,
    work: PathBuf,
}

impl Craft {
    /// The CLIs the host finds: `crafting.json` in `data_dir`, then `tools_dir`, then `PATH`.
    pub fn open(data_dir: &Path, tools_dir: &Path) -> Result<Self, CraftError> {
        let tools = Tools::discover(data_dir, tools_dir, env::var_os("PATH").as_deref())?;
        Ok(Self::new(
            tools,
            Arc::new(ProcessRunner),
            data_dir.join(WORK_DIR),
        ))
    }

    /// A host whose configuration cannot be read still starts: every document CLI is missing.
    pub fn without_tools(data_dir: &Path) -> Self {
        Self::new(
            Tools::none(),
            Arc::new(ProcessRunner),
            data_dir.join(WORK_DIR),
        )
    }

    pub fn new(tools: Tools, runner: Arc<dyn Runner + Send + Sync>, work: PathBuf) -> Self {
        Self {
            tools,
            runner,
            work,
        }
    }

    pub fn read(&self, doc: &Document, opts: &ReadOpts) -> Result<ReadOut, CraftError> {
        self.driver().read_text(doc, opts)
    }

    pub fn render(&self, doc: &Document, page: u32) -> Result<Preview, CraftError> {
        self.driver().render_png(doc, page)
    }

    /// The bytes of a new document made from text (DOC8).
    pub fn create(&self, kind: Kind, content: &str) -> Result<Vec<u8>, CraftError> {
        self.driver().create_file(kind, content)
    }

    fn driver(&self) -> Driver<'_> {
        Driver::new(&self.tools, self.runner.as_ref(), self.work.clone())
    }
}

impl DocumentPages for Craft {
    fn render_page(&self, source: &PageSource, page: u32) -> Result<RenderedPage, PageError> {
        let doc = Document {
            path: source.path.clone(),
            name: source.name.clone(),
            mime: source.mime.clone(),
        };
        let preview = self.render(&doc, page).map_err(page_error)?;
        Ok(RenderedPage {
            png: preview.png,
            pages: preview.pages,
        })
    }

    fn tools(&self) -> [DocTool; 3] {
        App::ALL.map(|app| DocTool {
            app: app.id(),
            name: app.name(),
            available: self.tools.available(app),
            tested_version: app.tested_version(),
        })
    }
}

/// The page answers the API knows; anything else is a failed render.
fn page_error(error: CraftError) -> PageError {
    let text = error.to_string();
    match error {
        CraftError::ToolMissing(_) => PageError::ToolMissing(text),
        CraftError::PageOutOfRange(_) => PageError::PageOutOfRange,
        CraftError::Unsupported(_) => PageError::Unsupported,
        _ => PageError::Failed,
    }
}
