//! A seat does not start without its approval hook (CORE9-F2). Claude skips a
//! hook it cannot run and then runs the action, so the seat refuses to start
//! instead of serving turns that nothing would gate.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hn_core_chat::ChatStore;
use hn_core_seat::{test_profile, SeatConfig, SeatService};
use hn_core_test_dir::TestDir;

#[test]
fn a_seat_with_a_missing_hook_does_not_start_and_names_the_hook() {
    let dir = TestDir::new("seat-missing-hook");
    let chat = Arc::new(ChatStore::open(&dir.path().join("chat.db")).expect("chat database"));
    let hook: PathBuf = dir.path().join("hn-uac-hook-absent");
    let config = SeatConfig {
        data_dir: dir.path().to_path_buf(),
        chat,
        mcp_command: dir.path().join("hn-mcp-absent"),
        tool_url: "http://127.0.0.1:9/internal/chat/tools".to_string(),
        hook_url: "http://127.0.0.1:9".to_string(),
        hook_command: hook.clone(),
        profiles: vec![test_profile("hn-no-such-cli-anywhere")],
        queue_capacity: 1,
        turn_timeout: Duration::from_secs(5),
    };

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let outcome = runtime.block_on(async { SeatService::start(config) });

    match outcome {
        Ok(_) => panic!("the seat started without its approval hook"),
        Err(error) => assert!(
            error.message.contains(&hook.display().to_string()),
            "the error does not name the hook: {error}"
        ),
    }
}
