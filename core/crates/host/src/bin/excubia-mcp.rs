//! `excubia-mcp` — the per-run MCP stdio server Alfred's CLI launches.
//!
//! It is a thin shim: only `chat.reply` and `chat.ask_user`, forwarded to the
//! host's loopback endpoint with the token inherited from the seat.
//!
//! `excubia-mcp resident -- <cli> <args…>` is a project seat for a CLI with no
//! resident mode of its own: it runs the CLI once per task the host types in.

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = match args.split_first() {
        Some((mode, rest)) if mode == "resident" => {
            let engine = rest.strip_prefix(&["--".to_string()]).unwrap_or(rest);
            excubia_seat::run_resident(engine).await
        }
        _ => excubia_seat::run_stdio().await,
    };
    if let Err(error) = outcome {
        eprintln!("excubia-mcp: {error}");
        std::process::exit(1);
    }
}
