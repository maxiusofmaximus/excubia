//! What the operator reads on a mail approval: the digest of a `mail.send` and
//! the batch of a `mail.move`. Only a device with `APPROVE` gets `details`.

use serde::{Deserialize, Serialize};

use super::MailAttachment;

/// The digest of a `mail.send` or `mail.move` approval, sent only to a device with
/// `APPROVE`. The fields are optional on the wire, so a row without them still reads.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MailApprovalDetails {
    #[serde(default)]
    pub account: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub body_preview: String,
    #[serde(default)]
    pub attachments: Vec<MailAttachment>,
    /// The mails of a `mail.move` batch; empty for a send.
    #[serde(default)]
    pub items: Vec<MailMoveItem>,
}

/// One mail of a `mail.move` batch. The sender and the subject are the mail's own
/// text, shown as text only. `excluded` is the host's mark once the operator left
/// the mail out of the batch.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct MailMoveItem {
    pub message_id: String,
    pub dest: String,
    pub reason: String,
    pub from_address: String,
    pub subject: String,
    pub level: String,
    pub excluded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn approval_details_read_with_missing_fields() {
        let details: MailApprovalDetails = serde_json::from_value(json!({
            "account": "trabajo",
            "to": ["bea@ejemplo.invalid"],
            "attachments": [{"name": "presupuesto.pdf", "size": 12, "sha256": "ab"}]
        }))
        .expect("details");
        assert_eq!(details.to, ["bea@ejemplo.invalid"]);
        assert!(details.subject.is_empty());
        assert_eq!(details.attachments[0].size, 12);
        assert!(details.items.is_empty());
    }

    #[test]
    fn a_move_batch_reads_its_items_and_the_host_marks() {
        let details: MailApprovalDetails = serde_json::from_value(json!({
            "account": "trabajo",
            "items": [
                {"message_id": "m1", "dest": "junkemail", "reason": "spam",
                 "from_address": "oferta@ejemplo.invalid", "subject": "Oferta", "level": "lead"},
                {"message_id": "m2", "dest": "inbox", "excluded": true}
            ]
        }))
        .expect("batch");
        assert_eq!(details.items.len(), 2);
        assert_eq!(details.items[0].dest, "junkemail");
        assert_eq!(details.items[0].level, "lead");
        assert!(!details.items[0].excluded);
        assert!(details.items[1].excluded);
        assert!(details.items[1].subject.is_empty());
    }
}
