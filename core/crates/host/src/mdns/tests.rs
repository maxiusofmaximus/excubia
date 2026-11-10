use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use excubia_core_api::{PakeSlot, PAKE_CODE_TTL, PAKE_PENDING_TTL};
use excubia_core_pake::Code;

use super::*;

type Log = Arc<Mutex<Vec<&'static str>>>;

/// Records every call; `fail_publish` makes the next publishes fail.
struct Fake {
    log: Log,
    fail_publish: Arc<Mutex<bool>>,
}

impl AnnounceBackend for Fake {
    fn publish(&mut self) -> Result<(), String> {
        self.log.lock().expect("log").push("publish");
        if *self.fail_publish.lock().expect("flag") {
            return Err("no network".to_owned());
        }
        Ok(())
    }
    fn withdraw(&mut self) -> Result<(), String> {
        self.log.lock().expect("log").push("withdraw");
        Ok(())
    }
    fn close(&mut self) {
        self.log.lock().expect("log").push("close");
    }
}

fn announcer() -> (Announcer<Fake>, Log, Arc<Mutex<bool>>) {
    let log = Log::default();
    let fail = Arc::default();
    let fake = Fake {
        log: Arc::clone(&log),
        fail_publish: Arc::clone(&fail),
    };
    (Announcer::new(fake), log, fail)
}

fn calls(log: &Log) -> Vec<&'static str> {
    std::mem::take(&mut *log.lock().expect("log"))
}

/// A slot whose clock only moves when the test says so.
fn slot_with_clock() -> (PakeSlot, Arc<Mutex<Duration>>) {
    let base = Instant::now();
    let offset = Arc::new(Mutex::new(Duration::ZERO));
    let seen = Arc::clone(&offset);
    let clock = Arc::new(move || base + *seen.lock().expect("clock"));
    (PakeSlot::with_clock(clock), offset)
}

fn tick(announcer: &mut Announcer<Fake>, slot: &PakeSlot) {
    announcer.sync(slot.is_live()).expect("sync");
}

#[test]
fn it_announces_once_per_live_period_and_withdraws_when_it_ends() {
    let (mut announcer, log, _) = announcer();
    let (slot, offset) = slot_with_clock();

    tick(&mut announcer, &slot);
    assert!(
        calls(&log).is_empty(),
        "nothing is announced without a code"
    );

    slot.arm(PAKE_CODE_TTL, "host").expect("arm");
    tick(&mut announcer, &slot);
    assert_eq!(calls(&log), ["publish"]);
    tick(&mut announcer, &slot);
    tick(&mut announcer, &slot);
    assert!(calls(&log).is_empty(), "a tick without change does nothing");

    *offset.lock().expect("clock") = PAKE_CODE_TTL;
    tick(&mut announcer, &slot);
    assert_eq!(calls(&log), ["withdraw"], "an expired code is withdrawn");
    tick(&mut announcer, &slot);
    assert!(calls(&log).is_empty(), "and only once");

    *offset.lock().expect("clock") = Duration::ZERO;
    slot.arm(PAKE_CODE_TTL, "host").expect("arm");
    tick(&mut announcer, &slot);
    assert_eq!(calls(&log), ["publish"], "a new code announces again");
}

#[test]
fn a_cleared_code_is_withdrawn() {
    let (mut announcer, log, _) = announcer();
    let slot = PakeSlot::new();
    slot.arm(PAKE_CODE_TTL, "host").expect("arm");
    tick(&mut announcer, &slot);
    slot.clear();
    tick(&mut announcer, &slot);
    assert_eq!(calls(&log), ["publish", "withdraw"]);
}

#[test]
fn the_pending_session_keeps_the_announcement_until_it_ends() {
    let (mut announcer, log, _) = announcer();
    let (slot, offset) = slot_with_clock();
    slot.arm(PAKE_CODE_TTL, "host").expect("arm");
    tick(&mut announcer, &slot);

    // What `start` does: burn the code, hold the session.
    let (code, _) = slot.take().expect("code");
    slot.hold_pending(a_key(&code), b"dev".to_vec());
    tick(&mut announcer, &slot);
    assert_eq!(calls(&log), ["publish"], "the ceremony is still running");

    *offset.lock().expect("clock") = PAKE_PENDING_TTL;
    tick(&mut announcer, &slot);
    assert_eq!(calls(&log), ["withdraw"]);
}

fn a_key(code: &Code) -> excubia_core_pake::SessionKey {
    use excubia_core_pake::{Role, Session};
    let (host, host_message) = Session::start(Role::Host, code, "host", b"dev").expect("host");
    let (device, device_message) = Session::start(Role::Device, code, "host", b"dev").expect("dev");
    drop(device.finish(&host_message));
    host.finish(&device_message).expect("key")
}

#[test]
fn shutdown_withdraws_and_closes() {
    let (mut announcer, log, _) = announcer();
    announcer.sync(true).expect("sync");
    announcer.shutdown().expect("shutdown");
    assert_eq!(calls(&log), ["publish", "withdraw", "close"]);

    announcer.shutdown().expect("again");
    assert_eq!(calls(&log), ["close"], "nothing left to withdraw");
}

#[test]
fn register_and_unregister_are_idempotent() {
    let (mut announcer, log, _) = announcer();
    announcer.unregister().expect("off already");
    announcer.register().expect("on");
    announcer.register().expect("on again");
    announcer.unregister().expect("off");
    announcer.unregister().expect("off again");
    assert_eq!(calls(&log), ["publish", "withdraw"]);
}

#[test]
fn a_failed_publish_is_reported_once_and_retried_in_the_next_live_period() {
    let (mut announcer, log, fail) = announcer();
    *fail.lock().expect("flag") = true;

    assert!(announcer.sync(true).is_err(), "the failure is reported");
    assert!(announcer.sync(true).is_ok(), "but not again every tick");
    assert_eq!(calls(&log), ["publish"], "and the publish is not retried");

    assert!(announcer.sync(false).is_ok());
    assert!(
        calls(&log).is_empty(),
        "nothing was on, nothing to withdraw"
    );

    *fail.lock().expect("flag") = false;
    assert!(announcer.sync(true).is_ok());
    assert_eq!(calls(&log), ["publish"], "the next code tries again");
}

#[tokio::test]
async fn the_task_announces_while_the_slot_is_live_and_withdraws_on_stop() {
    let (announcer, log, _) = announcer();
    let slot = Arc::new(PakeSlot::new());
    slot.arm(PAKE_CODE_TTL, "host").expect("arm");
    let task = spawn(Arc::clone(&slot), announcer);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(calls(&log), ["publish"]);

    task.stop().await;
    assert_eq!(calls(&log), ["withdraw", "close"]);
}
