//! The recipes of each oficio (F5): who keeps which text, under which limits.
//! Every text passes [`admit`] before any write, so a refused text never reaches
//! the table. Writes run under the store lock, so a check and its write cannot
//! interleave with another writer.

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::ChatError;
use crate::id;
use crate::model_recipe::{DraftSource, Recipe, RecipeText, MAX_RECIPES_PER_ROLE};
use crate::model_team::DelegationState;
use crate::recipe_guard::admit;
use crate::schema;

use super::company_id;
use super::delegation_rules::delegation_by_id;

const RECIPE_COLUMNS: &str = "id, role_id, title, body, uses, created_at, updated_at";

impl super::ChatStore {
    /// The recipes of one oficio, or of every oficio, oldest first.
    pub fn list_recipes(&self, role: Option<&str>) -> Result<Vec<Recipe>, ChatError> {
        let connection = self.lock();
        let clause = match role {
            Some(_) => "WHERE role_id = ?1 ORDER BY created_at, id",
            None => "ORDER BY created_at, id",
        };
        let mut statement = connection.prepare(&recipe_query(clause))?;
        let rows = statement.query_map(rusqlite::params_from_iter(role), recipe_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn recipe(&self, recipe_id: &str) -> Result<Recipe, ChatError> {
        let connection = self.lock();
        recipe_by_id(&connection, recipe_id)
    }

    /// Keep a new recipe for `role_id`. `confirm_risks` keeps a text with risks;
    /// a secret is refused whatever the answer.
    pub fn create_recipe(
        &self,
        role_id: &str,
        text: RecipeText,
        confirm_risks: bool,
    ) -> Result<Recipe, ChatError> {
        let text = admit(&text, confirm_risks)?;
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let kept: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM recipe WHERE role_id = ?1",
            [role_id],
            |row| row.get(0),
        )?;
        if kept >= MAX_RECIPES_PER_ROLE as i64 {
            return Err(ChatError::RecipeLimit);
        }
        if title_taken(&transaction, role_id, &text.title, None)? {
            return Err(ChatError::RecipeTitleTaken);
        }
        let now = schema::now(&transaction)?;
        let recipe_id = id::new_id("rcp");
        transaction.execute(
            "INSERT INTO recipe
                (id, company_id, role_id, title, body, uses, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?6)",
            params![
                recipe_id,
                company_id(&transaction)?,
                role_id,
                text.title,
                text.body,
                now
            ],
        )?;
        let recipe = recipe_by_id(&transaction, &recipe_id)?;
        transaction.commit()?;
        Ok(recipe)
    }

    /// Replace the title and the body of a recipe. Its oficio does not change.
    pub fn update_recipe(
        &self,
        recipe_id: &str,
        text: RecipeText,
        confirm_risks: bool,
    ) -> Result<Recipe, ChatError> {
        let text = admit(&text, confirm_risks)?;
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let current = recipe_by_id(&transaction, recipe_id)?;
        if title_taken(&transaction, &current.role_id, &text.title, Some(recipe_id))? {
            return Err(ChatError::RecipeTitleTaken);
        }
        let now = schema::now(&transaction)?;
        transaction.execute(
            "UPDATE recipe SET title = ?1, body = ?2, updated_at = ?3 WHERE id = ?4",
            params![text.title, text.body, now, recipe_id],
        )?;
        let recipe = recipe_by_id(&transaction, recipe_id)?;
        transaction.commit()?;
        Ok(recipe)
    }

    pub fn delete_recipe(&self, recipe_id: &str) -> Result<(), ChatError> {
        let connection = self.lock();
        let removed = connection.execute("DELETE FROM recipe WHERE id = ?1", [recipe_id])?;
        if removed == 0 {
            return Err(ChatError::NoSuchRecipe(recipe_id.to_string()));
        }
        Ok(())
    }

    /// Count one run that started. A recipe deleted meanwhile counts for nothing.
    pub fn count_recipe_use(&self, recipe_id: &str) -> Result<(), ChatError> {
        let connection = self.lock();
        connection.execute(
            "UPDATE recipe SET uses = uses + 1 WHERE id = ?1",
            [recipe_id],
        )?;
        Ok(())
    }

    /// What a draft is built from. Only a finished delegation of a hire makes one.
    pub fn recipe_draft_source(&self, delegation_id: &str) -> Result<DraftSource, ChatError> {
        let connection = self.lock();
        let delegation = delegation_by_id(&connection, delegation_id)?;
        if delegation.state != DelegationState::Done {
            return Err(ChatError::DelegationNotDone(delegation_id.to_string()));
        }
        let role_id: Option<String> = connection
            .query_row(
                "SELECT h.catalog_role FROM agent_seat s
                   JOIN agent_hire h ON h.id = s.hire_id
                  WHERE s.id = ?1",
                [&delegation.to_agent_id],
                |row| row.get(0),
            )
            .optional()?;
        let role_id =
            role_id.ok_or_else(|| ChatError::NoHireForRole(delegation.to_name.clone()))?;
        Ok(DraftSource {
            role_id,
            task: delegation.task,
        })
    }
}

/// Titles are one per oficio, compared without case. `except` is the recipe
/// being edited, which may keep its own title.
fn title_taken(
    connection: &Connection,
    role_id: &str,
    title: &str,
    except: Option<&str>,
) -> Result<bool, ChatError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM recipe
          WHERE role_id = ?1 AND lower(title) = lower(?2) AND id IS NOT ?3",
        params![role_id, title, except],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn recipe_by_id(connection: &Connection, recipe_id: &str) -> Result<Recipe, ChatError> {
    connection
        .query_row(&recipe_query("WHERE id = ?1"), [recipe_id], recipe_row)
        .optional()?
        .ok_or_else(|| ChatError::NoSuchRecipe(recipe_id.to_string()))
}

fn recipe_query(clause: &str) -> String {
    format!("SELECT {RECIPE_COLUMNS} FROM recipe {clause}")
}

fn recipe_row(row: &Row<'_>) -> rusqlite::Result<Recipe> {
    Ok(Recipe {
        id: row.get(0)?,
        role_id: row.get(1)?,
        title: row.get(2)?,
        body: row.get(3)?,
        uses: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}
