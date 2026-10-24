//! Live check against a running core host with `--tls-bind`. Ignored by
//! default: start the host, print a link with `host pair-code --lan`, and run
//! `HN_LIVE_LINK=<link> cargo test --test live_core_host -- --ignored`.
//! The wrong-pin probe goes first: a pin mismatch never reaches the host, so the
//! code stays unspent for the real pairing that follows.

use herdr_desktop_core::{pair_through_link, CoreError, DeviceIdentity, PairLink};

/// A valid fingerprint that is not the live host's (taken from the link tests).
const OTHER_PIN: &str = "h=-vJhhaleo1vBJMuBCljszQq8G6n02H6P2dSWaWP2jKI";

#[tokio::test]
#[ignore = "needs a running core host; set HN_LIVE_LINK"]
async fn a_live_host_refuses_a_stranger_then_pairs_through_its_link() {
    let live = std::env::var("HN_LIVE_LINK").expect("set HN_LIVE_LINK to a fresh pairing link");
    let identity = DeviceIdentity::generate("", "Escritorio en vivo");
    let key = identity.public_key_b64();

    let live_pin = live
        .split('&')
        .find(|part| part.starts_with("h="))
        .expect("the link carries a pin")
        .to_string();
    let stranger = live.replace(&live_pin, OTHER_PIN);
    let stranger = PairLink::parse_now(&stranger).expect("the probe link is well formed");
    let refused = pair_through_link(&stranger, &key, "Escritorio en vivo")
        .await
        .expect_err("a stranger's pin must not pair");
    assert!(matches!(refused, CoreError::PinMismatch), "{refused:?}");

    let link = PairLink::parse_now(&live).expect("the live link is current");
    let paired = pair_through_link(&link, &key, "Escritorio en vivo")
        .await
        .expect("the pinned host pairs over TLS 1.3");
    assert!(
        paired.base_url.starts_with("https://"),
        "{}",
        paired.base_url
    );
    assert!(!paired.response.device_id.is_empty());
    println!("live: pinned pairing ok at {}", paired.base_url);
}
