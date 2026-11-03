//! F2c against the real `mdns-sd`, in one process, on loopback: the announcer registers, a browser sees the
//! service with its port and TXT, the goodbye removes it, and a second live period announces it again.
//! It needs UDP multicast on the loopback interface; it is `#[ignore]` where that is not reliable.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use hn_core_host::mdns::{Announcer, MdnsBackend, SERVICE_TYPE};
use mdns_sd::{IfKind, Receiver, ResolvedService, ServiceDaemon, ServiceEvent};

const WAIT: Duration = Duration::from_secs(10);

/// The first event that `pick` accepts, or `None` after [`WAIT`].
fn wait_for<T>(
    browser: &Receiver<ServiceEvent>,
    pick: impl Fn(ServiceEvent) -> Option<T>,
) -> Option<T> {
    let deadline = Instant::now() + WAIT;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match browser.recv_timeout(left) {
            Ok(event) => {
                if let Some(found) = pick(event) {
                    return Some(found);
                }
            }
            Err(_) => return None,
        }
    }
    None
}

fn resolved(browser: &Receiver<ServiceEvent>) -> Option<Box<ResolvedService>> {
    wait_for(browser, |event| match event {
        ServiceEvent::ServiceResolved(info) => Some(info),
        _ => None,
    })
}

fn removed(browser: &Receiver<ServiceEvent>) -> Option<String> {
    wait_for(browser, |event| match event {
        ServiceEvent::ServiceRemoved(_, name) => Some(name),
        _ => None,
    })
}

#[test]
fn the_real_announcement_is_seen_with_its_txt_and_goodbye() {
    let tls_port = 47_219;
    let listener: SocketAddr = ([127, 0, 0, 1], tls_port).into();
    let mut announcer = Announcer::new(MdnsBackend::new(listener).expect("backend"));

    // The browser only listens on loopback, so nothing goes to the real network.
    let browser_daemon = ServiceDaemon::new().expect("browser daemon");
    browser_daemon.disable_interface(IfKind::All).expect("off");
    browser_daemon
        .enable_interface(IfKind::LoopbackV4)
        .expect("loopback");
    let browser = browser_daemon.browse(SERVICE_TYPE).expect("browse");

    announcer.sync(true).expect("announce");
    let info = resolved(&browser).expect("the announcement is seen");
    assert!(info.get_fullname().starts_with("excubia-"), "{info:?}");
    assert_eq!(info.get_port(), tls_port);
    let txt: Vec<_> = info
        .get_properties()
        .iter()
        .map(|p| (p.key().to_owned(), p.val_str().to_owned()))
        .collect();
    assert_eq!(
        txt,
        [("v".to_owned(), "1".to_owned())],
        "TXT is v=1 and nothing else"
    );

    announcer.sync(false).expect("withdraw");
    let gone = removed(&browser).expect("the goodbye is seen");
    assert_eq!(gone, info.get_fullname());

    announcer.sync(true).expect("announce again");
    let again = resolved(&browser).expect("the second announcement is seen");
    assert_eq!(again.get_fullname(), info.get_fullname());

    announcer.shutdown().expect("shutdown");
    assert!(removed(&browser).is_some(), "shutdown says goodbye too");
    let _ = browser_daemon.shutdown();
}
