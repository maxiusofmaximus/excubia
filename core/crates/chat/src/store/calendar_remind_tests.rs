use super::calendar_tests::{conversation, device, madrid, store_at, timed, MINUTE_MS, START_MS};
use crate::model::TimelineEntry;

#[test]
fn a_reminder_arrives_once_at_its_time_as_a_card_in_the_conversation() {
    let (_dir, store, clock) = store_at(START_MS);
    madrid(&store);
    let conversation_id = conversation(&store);
    let mut new = timed("Llamada", "2026-11-03T09:30");
    new.reminder_minutes = Some(30);
    device(&store, new);
    let fire_ms = ms_of("2026-11-03T08:00:00.000Z");
    clock.set(fire_ms - 30_000);
    assert_eq!(store.deliver_due_reminders().expect("early"), 0);
    clock.set(fire_ms);
    assert_eq!(store.deliver_due_reminders().expect("due"), 1);
    assert_eq!(store.deliver_due_reminders().expect("again"), 0);
    let page = store
        .timeline(&conversation_id, None, 20)
        .expect("timeline");
    let cards: Vec<&TimelineEntry> = page
        .entries
        .iter()
        .filter(|entry| entry.kind == "card")
        .collect();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].title.as_deref(), Some("Recordatorio: Llamada"));
    assert_eq!(cards[0].summary.as_deref(), Some("a las 09:30"));
}

/// A host that was off across a reminder delivers it on the next pass, up to 24 hours late;
/// an older one is dropped without a card.
#[test]
fn a_restart_delivers_the_recent_reminders_and_drops_the_old_ones() {
    let (_dir, store, clock) = store_at(START_MS);
    madrid(&store);
    let conversation_id = conversation(&store);
    let mut recent = timed("Reciente", "2026-11-03T09:30");
    recent.reminder_minutes = Some(0);
    device(&store, recent);
    let mut old = timed("Antigua", "2026-11-01T09:30");
    old.reminder_minutes = Some(0);
    device(&store, old);
    clock.set(ms_of("2026-11-03T08:30:00.000Z") + 5 * 60 * MINUTE_MS);
    assert_eq!(store.deliver_due_reminders().expect("catch up"), 1);
    let page = store
        .timeline(&conversation_id, None, 20)
        .expect("timeline");
    let titles: Vec<String> = page
        .entries
        .iter()
        .filter_map(|entry| entry.title.clone())
        .collect();
    assert_eq!(titles, ["Recordatorio: Reciente"]);
    assert_eq!(store.deliver_due_reminders().expect("none left"), 0);
}

#[test]
fn a_reminder_of_an_event_whose_conversation_is_gone_goes_to_the_latest_one() {
    let (_dir, store, clock) = store_at(START_MS);
    madrid(&store);
    let conversation_id = conversation(&store);
    let mut new = timed("Sin hilo", "2026-11-03T09:30");
    new.reminder_minutes = Some(0);
    device(&store, new);
    clock.set(ms_of("2026-11-03T08:30:00.000Z"));
    assert_eq!(store.deliver_due_reminders().expect("due"), 1);
    let page = store
        .timeline(&conversation_id, None, 20)
        .expect("timeline");
    assert!(page.entries.iter().any(|entry| entry.kind == "card"));
}

/// The UTC milliseconds of a stored instant, for the clock.
pub(super) fn ms_of(text: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(text)
        .expect("instant")
        .timestamp_millis()
}
