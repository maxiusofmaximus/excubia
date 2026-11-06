//! Recipes (F5): a text the person keeps per oficio and runs with one touch.
//! The text is plain and bounded here; what it may never hold is decided in
//! [`crate::recipe_guard`].

use std::sync::LazyLock;

use regex::Regex;

use crate::error::ChatError;

pub const MAX_RECIPES_PER_ROLE: usize = 50;
pub const RECIPE_TITLE_CHARS: usize = 80;
pub const RECIPE_BODY_CHARS: usize = 2000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recipe {
    pub id: String,
    pub role_id: String,
    pub title: String,
    pub body: String,
    pub uses: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// The text of a recipe as the person wrote it. [`check_text`] trims it and
/// refuses what the text rules forbid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecipeText {
    pub title: String,
    pub body: String,
}

/// What a run needs from the store: the body, the hire that takes it and the
/// seat that hands it over. The host spawns the panel and delivers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecipeRun {
    /// The recipe the text came from; `None` for a scheduled text of its own.
    pub recipe_id: Option<String>,
    pub conversation_id: String,
    pub hire_id: String,
    pub alfred_id: String,
    pub task: String,
    pub turn_key: String,
}

/// What a draft is built from: the task of a finished delegation and the oficio
/// of the hire that did it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftSource {
    pub role_id: String,
    pub task: String,
}

/// No HTML: a `<` that opens a tag or a declaration.
static HTML: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[A-Za-z/!?]").expect("html pattern"));

/// Trims the text and refuses what the text rules forbid, in a fixed order.
pub fn check_text(text: &RecipeText) -> Result<RecipeText, ChatError> {
    let title = text.title.trim().to_string();
    let body = text.body.trim().to_string();
    if title.is_empty() {
        return Err(ChatError::InvalidRecipe("title_empty"));
    }
    if title.chars().count() > RECIPE_TITLE_CHARS {
        return Err(ChatError::InvalidRecipe("title_too_long"));
    }
    if body.is_empty() {
        return Err(ChatError::InvalidRecipe("body_empty"));
    }
    if body.chars().count() > RECIPE_BODY_CHARS {
        return Err(ChatError::InvalidRecipe("body_too_long"));
    }
    if HTML.is_match(&title) || HTML.is_match(&body) {
        return Err(ChatError::InvalidRecipe("html_not_allowed"));
    }
    if title.chars().any(char::is_control)
        || body
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(ChatError::InvalidRecipe("control_characters"));
    }
    Ok(RecipeText { title, body })
}

/// The template a draft starts from: the oficio and the task, nothing read from
/// outside. The person reviews it before any save.
pub fn draft_text(role_title: &str, task: &str) -> RecipeText {
    let first_line = task.lines().next().unwrap_or_default();
    RecipeText {
        title: take_chars(&format!("{role_title}: {first_line}"), RECIPE_TITLE_CHARS),
        body: take_chars(
            &format!("Encargo: {task}\n\nAl terminar, devuelve un resumen breve del resultado."),
            RECIPE_BODY_CHARS,
        ),
    }
}

fn take_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}
