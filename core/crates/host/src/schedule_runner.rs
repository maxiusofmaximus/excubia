//! The scheduled runs (F4-CORE): each reminder tick claims the runs that are due and starts
//! them through the team seat. A run the host reaches late is never started: it is recorded
//! as missed and announced. The host never runs anything at its own startup.

use excubia_chat::{launch_schedule, ChatSeat, ChatStore};

/// One pass: every due run is launched, or recorded as skipped or missed. A failure is printed
/// and the pass goes on, so one broken schedule never holds back the others.
pub async fn pass<S: ChatSeat + ?Sized>(chat: &ChatStore, seat: &S) {
    let launches = match chat.claim_due_schedules() {
        Ok(launches) => launches,
        Err(error) => {
            eprintln!("host: could not claim the due schedules: {error}");
            return;
        }
    };
    for launch in launches {
        if let Err(error) = launch_schedule(chat, seat, launch).await {
            eprintln!("host: a scheduled run did not start: {error}");
        }
    }
}
