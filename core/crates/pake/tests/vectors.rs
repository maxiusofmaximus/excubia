//! Deterministic vector produced by python-spake2 0.9 (`tests/interop/interop.py vector`, Ed25519 group): the Rust crate
//! must give the same messages, the same key and the same confirmation tags from the same scalars.
//! These are NOT RFC 9382 vectors: `spake2` 0.4.0 is not RFC 9382 (see PAIRING-F2A-RESULT.md).

use excubia_pake::{Code, Role, Session};

const CODE: &str = "K7M2QXTZ";
const HOST_ID: &str = "excubia-host-1";
const DEVICE_PUBKEY: &str = "040102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f40";
const EXPORTER: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const SPKI: &str = "6465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f80818283";
const DEVICE_ENTROPY: &str = "929358423de86e7c1060ffcf89e3082714b40c4e48fa866b9f044c30d6c9a2010000000000000000000000000000000000000000000000000000000000000000";
const DEVICE_MESSAGE: &str = "41941a360e1a14d8f365f2e713f998fee9ddaa74421489cf459fab37eed117b875";
const HOST_ENTROPY: &str = "5c5841baac8fe8652a2afda1bf9139b977714972f6f0634e56277de7c519130a0000000000000000000000000000000000000000000000000000000000000000";
const HOST_MESSAGE: &str = "4262dbd524edc6cd02ddf973e46baca7d27a8db7f27136d370fd3f2512a6d7937e";
const KEY: &str = "7f623fc2f7227668698051d6513219a5b55213546084aa4b1d12b8045dd57f31";
const HOST_TAG: &str = "b411a9ae705f559139bf3a6f9197d27bbf1223e786506d41ded08d1cec07846a";
const DEVICE_TAG: &str = "81d1b56f93a22828f50f11e5b710ac48dc5fad5402936f0c8191235ae6de4c88";

fn hex<const N: usize>(text: &str) -> [u8; N] {
    data_encoding::HEXLOWER
        .decode(text.as_bytes())
        .expect("hex")
        .try_into()
        .expect("length")
}

fn hexv(text: &str) -> Vec<u8> {
    data_encoding::HEXLOWER
        .decode(text.as_bytes())
        .expect("hex")
}

#[test]
fn both_roles_match_python_spake2() {
    let code = Code::parse(CODE).expect("code");
    let pubkey = hexv(DEVICE_PUBKEY);
    let (exporter, spki): ([u8; 32], [u8; 32]) = (hex(EXPORTER), hex(SPKI));

    let (device, device_msg) =
        Session::start_with_entropy(Role::Device, &code, HOST_ID, &pubkey, hex(DEVICE_ENTROPY))
            .expect("device");
    let (host, host_msg) =
        Session::start_with_entropy(Role::Host, &code, HOST_ID, &pubkey, hex(HOST_ENTROPY))
            .expect("host");
    assert_eq!(device_msg, hexv(DEVICE_MESSAGE));
    assert_eq!(host_msg, hexv(HOST_MESSAGE));

    let device_key = device.finish(&host_msg).expect("device finish");
    let host_key = host.finish(&device_msg).expect("host finish");
    assert_eq!(device_key.expose_for_tests(), hex::<32>(KEY));
    assert_eq!(host_key.expose_for_tests(), hex::<32>(KEY));
    assert_eq!(
        host_key
            .confirm_tag(Role::Host, &exporter, &spki)
            .expect("tag"),
        hex::<32>(HOST_TAG)
    );
    assert_eq!(
        device_key
            .confirm_tag(Role::Device, &exporter, &spki)
            .expect("tag"),
        hex::<32>(DEVICE_TAG)
    );
}
