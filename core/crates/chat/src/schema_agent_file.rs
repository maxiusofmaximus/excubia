//! The run that made an attachment (DOC8). A file Alfred creates names its run, so a
//! turn can deliver the files its reply left out; the files users send keep it empty.

use rusqlite::Connection;

use crate::error::ChatError;
use crate::schema_approval::add_column_once;

pub fn migrate_to_26(connection: &mut Connection) -> Result<(), ChatError> {
    let transaction = connection.transaction()?;
    add_column_once(&transaction, "attachment", "created_run", "TEXT")?;
    transaction.execute_batch(
        "CREATE INDEX IF NOT EXISTS attachment_created_run
             ON attachment(conversation_id, created_run);",
    )?;
    transaction.pragma_update(None, "user_version", 26)?;
    transaction.commit()?;
    Ok(())
}
