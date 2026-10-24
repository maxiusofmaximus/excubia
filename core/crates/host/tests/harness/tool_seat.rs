//! The tool seat of the harness: an MCP endpoint that answers with the real
//! parser's typed errors, and the agent catalogue every test host writes.

#![allow(dead_code)]

use hn_core_chat::{ChatSeat, SeatError, ToolError, ToolReply, ToolRequest, Turn};
use hn_core_host::paths;

use super::TOOL_TOKEN;

/// A seat with no chat turn, only an MCP endpoint: it answers with the real
/// parser's typed errors, so the HTTP shape of an invalid tool call is
/// testable without a live CLI.
pub(super) struct ToolSeat;

#[async_trait::async_trait]
impl ChatSeat for ToolSeat {
    fn try_enqueue(&self, _turn: Turn) -> Result<(), SeatError> {
        Err(SeatError::unavailable("the tool seat has no chat turn"))
    }

    async fn tool(&self, token: &str, request: ToolRequest) -> Result<ToolReply, ToolError> {
        if token != TOOL_TOKEN {
            return Err(ToolError::unauthorized("unknown or expired tool token"));
        }
        match hn_core_seat::parse_tool(&request) {
            Ok(_) => Ok(ToolReply {
                result: serde_json::json!({ "status": "accepted" }),
            }),
            Err(error) => Err(error),
        }
    }
}

/// The catalogue every test host uses: `fake` (this test binary, available)
/// and `missing` (a name nothing installs, unavailable). The client can name
/// them, never a path.
pub(super) fn write_catalog(data_dir: &std::path::Path) {
    let fake = env!("CARGO_BIN_EXE_fake-agent");
    let text = format!(
        "[agents.fake]\ncommand = '{fake}'\nargs = [\"--delay-ms\", \"150\"]\nlabel = \"Fake agent\"\n\
         \n[agents.missing]\ncommand = 'hn-no-such-binary-anywhere'\n"
    );
    std::fs::write(paths::agents_path(data_dir), text).expect("agents.toml");
}
