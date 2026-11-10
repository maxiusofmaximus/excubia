//! Rust <-> python-spake2 in both directions, with fresh randomness on both sides.
//! Ignored by default because it needs a Python with `spake2`: run
//! `EXCUBIA_PAKE_PYTHON=<venv python> cargo test -p excubia-core-pake --test interop -- --ignored`.

use std::process::Command;

use excubia_core_pake::{Code, Role, Session};

const HOST_ID: &str = "excubia-host-1";
const DEVICE_PUBKEY: [u8; 65] = [4; 65];
const EXPORTER: [u8; 32] = [7; 32];
const SPKI: [u8; 32] = [9; 32];

fn hex(bytes: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(bytes)
}

fn unhex(text: &str) -> Vec<u8> {
    data_encoding::HEXLOWER
        .decode(text.as_bytes())
        .expect("hex")
}

/// Python plays `python_role` against the Rust side, which plays `rust_role`.
fn interoperate(python_role: &str, rust_role: Role) {
    let python =
        std::env::var("EXCUBIA_PAKE_PYTHON").expect("EXCUBIA_PAKE_PYTHON must point to a python with spake2");
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/interop/interop.py");
    let code = Code::generate().expect("code");
    let (session, message) =
        Session::start(rust_role, &code, HOST_ID, &DEVICE_PUBKEY).expect("start");
    let output = Command::new(python)
        .args(["-I", script, "peer", python_role, code.reveal(), HOST_ID])
        .args([
            hex(&DEVICE_PUBKEY),
            hex(&message),
            hex(&EXPORTER),
            hex(&SPKI),
        ])
        .output()
        .expect("run python");
    assert!(
        output.status.success(),
        "python failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("utf8");
    let field = |name: &str| -> String {
        let line = text
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{name}=")));
        line.unwrap_or_else(|| panic!("python printed no {name}"))
            .to_owned()
    };

    let key = session
        .finish(&unhex(&field("message")))
        .expect("rust finish");
    let python_peer = if python_role == "host" {
        Role::Host
    } else {
        Role::Device
    };
    key.verify_confirm(python_peer, &EXPORTER, &SPKI, &unhex(&field("tag")))
        .expect("python's tag verifies in Rust");
    assert_eq!(
        hex(&key.expose_for_tests()),
        field("key"),
        "both implementations derived the same key"
    );
}

#[test]
#[ignore = "needs EXCUBIA_PAKE_PYTHON (a python with spake2 installed)"]
fn rust_host_with_python_device() {
    interoperate("device", Role::Host);
}

#[test]
#[ignore = "needs EXCUBIA_PAKE_PYTHON (a python with spake2 installed)"]
fn rust_device_with_python_host() {
    interoperate("host", Role::Device);
}
