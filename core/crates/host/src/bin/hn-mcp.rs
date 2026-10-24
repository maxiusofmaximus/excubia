//! `hn-mcp` — the per-run MCP stdio server Alfred's CLI launches.
//!
//! It is a thin shim: only `chat.reply` and `chat.ask_user`, forwarded to the
//! host's loopback endpoint with the token inherited from the seat.

#[tokio::main]
async fn main() {
    if let Err(error) = hn_core_seat::run_stdio().await {
        eprintln!("hn-mcp: {error}");
        std::process::exit(1);
    }
}
