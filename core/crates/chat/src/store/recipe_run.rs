//! Starting a recipe (F5). The store checks what a run needs and names the hire,
//! Alfred and the body. The host spawns the hire's panel and delivers the work.

use rusqlite::{Connection, OptionalExtension};

use crate::error::ChatError;
use crate::id;
use crate::model_recipe::RecipeRun;

use super::conversation_exists;

/// The run a text starts in a conversation: the conversation must be in a project and the
/// oficio must have a hired hire. Shared by recipes and scheduled work.
pub(super) fn run_of(
    connection: &Connection,
    alfred_id: String,
    conversation_id: &str,
    role_id: &str,
    task: String,
    recipe_id: Option<String>,
) -> Result<RecipeRun, ChatError> {
    if !conversation_exists(connection, conversation_id)? {
        return Err(ChatError::NoSuchConversation);
    }
    let project: Option<String> = connection.query_row(
        "SELECT project_id FROM conversation WHERE id = ?1",
        [conversation_id],
        |row| row.get(0),
    )?;
    if project.is_none() {
        return Err(ChatError::ConversationWithoutProject);
    }
    let hire_id: Option<String> = connection
        .query_row(
            "SELECT id FROM agent_hire WHERE catalog_role = ?1 AND status = 'hired'",
            [role_id],
            |row| row.get(0),
        )
        .optional()?;
    let hire_id = hire_id.ok_or_else(|| ChatError::NoHireForRole(role_id.to_string()))?;
    Ok(RecipeRun {
        recipe_id,
        conversation_id: conversation_id.to_string(),
        hire_id,
        alfred_id,
        task,
        turn_key: id::new_id("rcp"),
    })
}

impl super::ChatStore {
    /// The run of a recipe in a conversation, or the reason it cannot start: the
    /// recipe is gone, the conversation is not in a project, or no hire of the
    /// oficio is contracted.
    pub fn recipe_run_source(
        &self,
        recipe_id: &str,
        conversation_id: &str,
    ) -> Result<RecipeRun, ChatError> {
        let recipe = self.recipe(recipe_id)?;
        let alfred_id = self.alfred_id()?;
        let connection = self.lock();
        run_of(
            &connection,
            alfred_id,
            conversation_id,
            &recipe.role_id,
            recipe.body,
            Some(recipe.id),
        )
    }
}
