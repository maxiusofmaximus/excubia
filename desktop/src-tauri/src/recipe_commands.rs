//! «Recetas» commands: the reusable texts of a professional, read and decided from the screen.
//! Each one is a signed call to the host; a refusal comes back as a typed `ChatFailure` the
//! screen words by its `code`. Saving always follows a review in the screen, never a command of its own.

use herdr_desktop_core::chat::{ChatFailure, Recipe, RecipeDraft, RecipeRun};
use tauri::State;

use crate::chat_commands::{client, shared, Shared};

#[tauri::command]
pub async fn recipes(state: State<'_, Shared>, role: String) -> Result<Vec<Recipe>, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.recipes(&role).await
}

#[tauri::command]
pub async fn recipe_draft(
    state: State<'_, Shared>,
    delegation_id: String,
) -> Result<RecipeDraft, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.recipe_draft(&delegation_id).await
}

#[tauri::command]
pub async fn recipe_create(
    state: State<'_, Shared>,
    role: String,
    title: String,
    body: String,
    confirm_risks: bool,
) -> Result<Recipe, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?
        .recipe_create(&role, &title, &body, confirm_risks)
        .await
}

#[tauri::command]
pub async fn recipe_update(
    state: State<'_, Shared>,
    id: String,
    title: String,
    body: String,
    confirm_risks: bool,
) -> Result<Recipe, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?
        .recipe_update(&id, &title, &body, confirm_risks)
        .await
}

#[tauri::command]
pub async fn recipe_delete(state: State<'_, Shared>, id: String) -> Result<String, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.recipe_delete(&id).await
}

#[tauri::command]
pub async fn recipe_run(
    state: State<'_, Shared>,
    id: String,
    conversation_id: String,
) -> Result<RecipeRun, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.recipe_run(&id, &conversation_id).await
}
