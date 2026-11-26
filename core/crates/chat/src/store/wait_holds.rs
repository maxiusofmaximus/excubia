//! Waits the host holds open for a person outside the approval cards: the GitHub code
//! the operator types on github.com. A held wait counts as one pending wait of its
//! approval's run and delegation, so the turn's and the delegation's active-time budget
//! pauses on it as it does on a card. Same budget, no second clock.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rusqlite::OptionalExtension;
use tokio::sync::watch;

use super::ChatStore;
use crate::error::ChatError;

/// The column a budget counts a wait under, with the id it counts for.
type Key = (&'static str, String);

/// The holds in force, one count per key, and a change counter the budget waits on.
pub(super) struct Holds {
    counts: Mutex<Vec<(Key, usize)>>,
    /// Bumped at every change, so a budget wait reads the waits again.
    changes: watch::Sender<u64>,
}

impl Holds {
    pub(super) fn new() -> Self {
        Self {
            counts: Mutex::new(Vec::new()),
            changes: watch::channel(0).0,
        }
    }

    /// How many holds count under `column` for `id`; `column` is a fixed literal.
    pub(super) fn count(&self, column: &str, id: &str) -> usize {
        lock(&self.counts)
            .iter()
            .find(|((c, v), _)| *c == column && v == id)
            .map_or(0, |(_, n)| *n)
    }

    fn subscribe(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    fn move_counts(&self, keys: &[Key], up: bool) {
        if keys.is_empty() {
            return;
        }
        {
            let mut counts = lock(&self.counts);
            for key in keys {
                match counts.iter().position(|(k, _)| k == key) {
                    Some(at) if up => counts[at].1 += 1,
                    Some(at) => {
                        counts[at].1 -= 1;
                        if counts[at].1 == 0 {
                            counts.swap_remove(at);
                        }
                    }
                    None if up => counts.push((key.clone(), 1)),
                    None => {}
                }
            }
        }
        // `send_modify` changes the value even with no receiver; `send` would not.
        self.changes.send_modify(|version| *version += 1);
    }
}

/// One hold on a run's and a delegation's wait. It counts only while it is on, and it
/// is released when it is set off or dropped, so a wait that ends early never leaks.
pub struct WaitHold {
    holds: Arc<Holds>,
    keys: Vec<Key>,
    on: bool,
}

impl WaitHold {
    /// Holds the budget (`true`) or lets it run (`false`). The same state twice changes nothing.
    pub fn set(&mut self, on: bool) {
        if on != self.on {
            self.on = on;
            self.holds.move_counts(&self.keys, on);
        }
    }
}

impl Drop for WaitHold {
    fn drop(&mut self) {
        self.set(false);
    }
}

impl ChatStore {
    /// A hold on the run and on the delegation that the approval `approval_id` belongs
    /// to. An approval without a run or a delegation holds nothing for that side.
    pub fn hold_approval_wait(&self, approval_id: &str) -> Result<WaitHold, ChatError> {
        let scope: Option<(Option<String>, Option<String>)> = self
            .lock()
            .query_row(
                "SELECT run_id, delegation_id FROM approval WHERE id = ?1",
                [approval_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let keys: Vec<Key> = match scope {
            Some((run, delegation)) => run
                .into_iter()
                .filter(|id| !id.is_empty())
                .map(|id| ("approval.run_id", id))
                .chain(
                    delegation
                        .into_iter()
                        .map(|id| ("approval.delegation_id", id)),
                )
                .collect(),
            None => Vec::new(),
        };
        Ok(WaitHold {
            holds: Arc::clone(&self.holds),
            keys,
            on: false,
        })
    }

    /// Receives a change each time a hold is set on or off, so a budget reads its waits again.
    pub fn wait_holds_changed(&self) -> watch::Receiver<u64> {
        self.holds.subscribe()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
