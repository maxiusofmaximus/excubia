//! The reminders' clock (CALENDARIO-F4): every 30 seconds the host delivers the reminders
//! that have come due, as cards in the chat. Each reminder arrives once. One that came due
//! while the host was down arrives at the first tick, unless it is more than a day old.
//! A delivered card also sends one live `alert` frame when a rule (F5) allows it; the card
//! itself is never muted, and a reminder never passes the brakes of `alert_policy`.

use std::sync::Arc;
use std::time::Duration;

use hn_core_chat::{AlertDecision, ChatSeat, ChatStore};
use hn_core_state::StateStore;

use crate::alerts::{self, AlertKind};
use crate::schedule_runner;

/// How often the reminders are checked, so a reminder arrives at most this late.
pub const REMINDER_EVERY: Duration = Duration::from_secs(30);
/// The most `alert` frames one pass sends; the cards all arrive, so a host that was down
/// for a day does not buzz the phone once per reminder it missed.
const MAX_FRAMES_PER_PASS: usize = 3;

/// Delivers the due reminders and starts the due scheduled runs, for as long as the host lives.
/// Needs a Tokio runtime.
pub fn watch(chat: Arc<ChatStore>, store: Arc<StateStore>, seat: Arc<dyn ChatSeat>) {
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(REMINDER_EVERY);
        loop {
            ticks.tick().await;
            deliver(&chat, &store);
            schedule_runner::pass(&chat, seat.as_ref()).await;
        }
    });
}

/// One pass: the due reminders become cards, and each card that a rule allows also becomes
/// an `alert` frame, up to `MAX_FRAMES_PER_PASS`. The frames follow the cards they announce.
pub fn deliver(chat: &ChatStore, store: &StateStore) {
    let cards = match chat.deliver_due_reminders() {
        Ok(cards) => cards,
        Err(error) => {
            eprintln!("host: could not deliver the due reminders: {error}");
            return;
        }
    };
    if cards == 0 {
        return;
    }
    let rule_id = match chat.alert_for_reminder() {
        Ok(AlertDecision::Alert { rule_id }) => rule_id,
        Ok(AlertDecision::Silent) => return,
        Err(error) => {
            eprintln!("host: could not read the alert rules: {error}");
            return;
        }
    };
    let ts = chat.now_text();
    for _ in 0..cards.min(MAX_FRAMES_PER_PASS) {
        alerts::publish(store, AlertKind::CalendarReminder, rule_id.clone(), &ts);
    }
}
