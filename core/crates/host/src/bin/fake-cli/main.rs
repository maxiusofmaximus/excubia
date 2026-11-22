//! `fake-cli` — a simulated Alfred CLI for the CORE6B/CORE7 acceptance tests.
//!
//! It is a real child process that reads the prompt from stdin, launches the
//! configured MCP server over stdio and drives `chat.reply`/`chat.ask_user`.
//! The test picks a scenario with a marker in the user's message:
//! `[[fake:silent]]`, `[[fake:hang]]`, `[[fake:invalid]]`, `[[fake:inject]]`,
//! `[[fake:ask]]`, `[[fake:team]]` or the default reply. It never spends model
//! quota.
//!
//! With `--resident` it becomes a project seat (CORE7): one process per panel
//! that reads tasks from stdin and answers with `delegation.complete`,
//! `delegation.assign` and `agent.spawn`.

mod mcp;
mod resident;
mod run;
mod scenario;
mod uac;

use run::run;

fn main() {
    if let Err(error) = run() {
        eprintln!("fake-cli: {error}");
        std::process::exit(2);
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}
