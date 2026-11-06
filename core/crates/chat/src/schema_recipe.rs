//! The recipe table (F5): one row per text a person keeps for an oficio. `uses`
//! counts the runs that started; the unique index keeps one title per oficio.

use rusqlite::Connection;

use crate::error::ChatError;

pub const RECIPE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS recipe (
    id          TEXT PRIMARY KEY,
    company_id  TEXT NOT NULL REFERENCES company(id),
    role_id     TEXT NOT NULL,
    title       TEXT NOT NULL,
    body        TEXT NOT NULL,
    uses        INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS recipe_role_title ON recipe(role_id, lower(title));
CREATE INDEX IF NOT EXISTS recipe_role ON recipe(role_id, created_at);
"#;

pub fn migrate_to_23(connection: &mut Connection) -> Result<(), ChatError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(RECIPE_SCHEMA)?;
    transaction.pragma_update(None, "user_version", 23)?;
    transaction.commit()?;
    Ok(())
}
