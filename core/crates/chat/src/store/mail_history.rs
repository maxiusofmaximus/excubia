//! What the operator has decided about each sender domain of the mail batches (`mail.move`):
//! how many of its messages he let go to the bin and how many he kept (left out, denied).
//! It is read from the decided approvals, so it needs no table and holds no subject.

use std::collections::HashMap;

use serde_json::Value;

use crate::error::ChatError;
use crate::store::ChatStore;

/// Decisions on one sender domain.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SenderHistory {
    pub deleted: u32,
    pub kept: u32,
}

impl ChatStore {
    /// Per lowercase sender domain, the operator's decisions on moved batches.
    pub fn mail_sender_history(&self) -> Result<HashMap<String, SenderHistory>, ChatError> {
        let connection = self.lock();
        let mut statement = connection.prepare(
            "SELECT status, payload_redacted FROM approval
              WHERE tool = 'mail.move' AND status IN ('consumed', 'denied')",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        let mut history: HashMap<String, SenderHistory> = HashMap::new();
        for row in rows {
            let (status, payload) = row?;
            let payload: Value = payload
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or(Value::Null);
            for item in payload["items"].as_array().into_iter().flatten() {
                let Some(domain) = item["from_address"]
                    .as_str()
                    .and_then(|address| address.rsplit_once('@'))
                    .map(|(_, domain)| domain.to_ascii_lowercase())
                else {
                    continue;
                };
                let entry = history.entry(domain).or_default();
                if status == "denied" || item["excluded"] == true {
                    entry.kept += 1;
                } else {
                    entry.deleted += 1;
                }
            }
        }
        Ok(history)
    }
}
