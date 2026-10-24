//! What `host devices import-gateway` prints: one line per device, with the id
//! masked and never a key or a hash.

use hn_core_auth::{ImportEntry, ImportOutcome};

/// The id as it may appear on a screen: enough to tell two devices apart, not
/// enough to reuse one.
pub fn masked_id(device_id: &str) -> String {
    /// Shorter ids are hidden whole: a few trailing characters would name them.
    const SHOW_ENDS_FROM: usize = 13;
    let characters: Vec<char> = device_id.chars().collect();
    match characters.len() {
        0 => "(no id)".to_string(),
        len if len < SHOW_ENDS_FROM => "****".to_string(),
        len => {
            let head: String = characters[..4].iter().collect();
            let tail: String = characters[len - 4..].iter().collect();
            format!("{head}...{tail}")
        }
    }
}

pub fn report_lines(entries: &[ImportEntry]) -> Vec<String> {
    entries.iter().map(entry_line).collect()
}

pub fn summary(entries: &[ImportEntry]) -> String {
    let count = |wanted: fn(&ImportOutcome) -> bool| {
        entries
            .iter()
            .filter(|entry| wanted(&entry.outcome))
            .count()
    };
    format!(
        "added {}, replaced {}, unchanged {}, conflicts {} (use --force to replace), rejected {}",
        count(|outcome| matches!(outcome, ImportOutcome::Added)),
        count(|outcome| matches!(outcome, ImportOutcome::Replaced)),
        count(|outcome| {
            matches!(
                outcome,
                ImportOutcome::AlreadyPresent | ImportOutcome::DuplicateKey { .. }
            )
        }),
        count(|outcome| matches!(outcome, ImportOutcome::KeyConflict)),
        count(|outcome| matches!(outcome, ImportOutcome::Rejected(_))),
    )
}

fn entry_line(entry: &ImportEntry) -> String {
    let verdict = match &entry.outcome {
        ImportOutcome::Added => "added".to_string(),
        ImportOutcome::AlreadyPresent => "already here with the same key, unchanged".to_string(),
        ImportOutcome::DuplicateKey { known_as } => format!(
            "same key already registered as {}, skipped",
            masked_id(known_as)
        ),
        ImportOutcome::KeyConflict => {
            "WARNING: this id is registered with another key, kept (use --force to replace it)"
                .to_string()
        }
        ImportOutcome::Replaced => "WARNING: replaced the registered key (--force)".to_string(),
        ImportOutcome::Rejected(error) => format!("rejected: {error}"),
    };
    let id = masked_id(&entry.device_id);
    let label = entry.label.as_deref().unwrap_or("-");
    if entry.unknown_permissions.is_empty() {
        format!("{id}  {label}  {verdict}")
    } else {
        format!(
            "{id}  {label}  {verdict}; unknown permissions ignored: {}",
            entry.unknown_permissions.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hn_core_auth::RecordError;

    fn entry(outcome: ImportOutcome) -> ImportEntry {
        ImportEntry {
            device_id: "dev-0123456789abcdef".into(),
            label: Some("Tablet".into()),
            outcome,
            unknown_permissions: Vec::new(),
        }
    }

    #[test]
    fn ids_are_masked() {
        assert_eq!(masked_id("dev-0123456789abcdef"), "dev-...cdef");
        assert_eq!(masked_id("dev-synth-a1"), "****");
        assert_eq!(masked_id(""), "(no id)");
    }

    #[test]
    fn the_report_never_shows_a_whole_id_and_the_summary_counts_outcomes() {
        let entries = vec![
            entry(ImportOutcome::Added),
            entry(ImportOutcome::KeyConflict),
            entry(ImportOutcome::AlreadyPresent),
            entry(ImportOutcome::DuplicateKey {
                known_as: "dev-fedcba9876543210".into(),
            }),
            entry(ImportOutcome::Rejected(RecordError::MissingDeviceId)),
        ];
        let lines = report_lines(&entries);
        assert!(lines[0].contains("Tablet"));
        assert!(lines[1].contains("--force"));
        assert!(lines.iter().all(|line| !line.contains("0123456789abcdef")));
        assert!(lines[3].contains("dev-...3210"));
        assert_eq!(
            summary(&entries),
            "added 1, replaced 0, unchanged 2, conflicts 1 (use --force to replace), rejected 1"
        );
    }
}
