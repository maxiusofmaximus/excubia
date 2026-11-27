//! The lead's panel is a PTY child of the host, and the daemon puts the toolchain folders on
//! PATH at startup. This proves a panel child inherits them, so git and gh resolve by name
//! there. One test per binary: it changes the process environment.
#![cfg(windows)]

use std::time::Duration;

use excubia_pty::{CommandSpec, PtySession, PtySize};
use excubia_test_dir::TestDir;
use excubia_toolchain::{pinned, Layout};

#[tokio::test]
async fn a_panel_child_finds_the_toolchain_folders_on_path() {
    let dir = TestDir::new("seat-path");
    let layout = Layout::new(&dir.join("data"));
    let dists = pinned();
    // The daemon's startup step, before any panel is spawned.
    layout.apply_to_process(&dists).expect("startup PATH");
    let wanted: Vec<String> = layout
        .path_dirs(&dists)
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();

    let spec = CommandSpec::new("cmd.exe").arg("/C").arg("echo %PATH%");
    let mut session = PtySession::spawn(spec, PtySize { rows: 40, cols: 4000 }).expect("spawn");
    let mut output = session.take_output().expect("output");
    let mut exit_rx = session.take_exit().expect("exit");
    let mut seen = Vec::new();
    let mut exited = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let text = String::from_utf8_lossy(&seen).into_owned();
        if exited && wanted.iter().all(|dir| text.contains(dir.as_str())) {
            break;
        }
        tokio::select! {
            chunk = output.recv() => match chunk {
                Some(bytes) => {
                    // ConPTY asks for the cursor position before it streams the child's output.
                    if bytes.windows(4).any(|window| window == b"\x1b[6n") {
                        session.write(b"\x1b[1;1R").expect("cursor reply");
                    }
                    seen.extend_from_slice(&bytes);
                }
                None => break,
            },
            info = exit_rx.recv(), if !exited => {
                let _ = info;
                exited = true;
            }
            _ = tokio::time::sleep_until(deadline) => {
                panic!("the panel child did not finish: {:?}", String::from_utf8_lossy(&seen))
            }
        }
    }

    let text = String::from_utf8_lossy(&seen);
    for folder in &wanted {
        assert!(text.contains(folder.as_str()), "{folder} is not on the panel PATH: {text:?}");
    }
}
