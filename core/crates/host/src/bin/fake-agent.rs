//! `fake-agent` — a scripted CLI agent for tests and the manual demo.
//!
//! `approval` (default): prints a busy line, then an approval dialog shaped
//! like Claude Code's (with the `Esc to cancel · Enter to confirm` footer the
//! manifests match), waits for a line, echoes what arrived, prints `terminado`
//! and then stays at a prompt, answering every further line until told to
//! leave (`salir` / `exit`). Staying alive is what makes the `done` state
//! observable: the agent finished, the operator has not looked yet.
//!
//! `quick`: prints the busy line and `terminado`, then exits — the pane is
//! gone, which is the dead-pane case.
//!
//! `--delay-ms N` tunes the busy phase (default 200 ms).

use std::io::{BufRead, Write};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scenario = flag(&args, "--scenario").unwrap_or_else(|| "approval".to_string());
    let delay_ms = flag(&args, "--delay-ms")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(200);

    println!("Trabajando en la tarea...");
    std::io::stdout().flush().ok();
    std::thread::sleep(std::time::Duration::from_millis(delay_ms));

    if scenario == "quick" {
        println!("terminado");
        std::io::stdout().flush().ok();
        return;
    }

    println!();
    println!("╭──────────────────────────────────────────╮");
    println!("│ ¿Quieres continuar?                      │");
    println!("│ ❯ 1. Sí                                  │");
    println!("│   2. No                                  │");
    println!("╰──────────────────────────────────────────╯");
    println!("────────────────────────────────────────────");
    println!("  Esc to cancel · Enter to confirm");
    std::io::stdout().flush().ok();

    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    println!("RECIBIDO: {}", line.trim());
    println!("terminado");
    // A fresh rule closes the dialog: the live region after the last rule is
    // the prompt again, which is what the detection rules read.
    println!("────────────────────────────────────────────");

    // Stay alive at a prompt: `done` (idle + unseen) needs a live pane.
    let stdin = std::io::stdin();
    loop {
        print!("❯ ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).is_err() {
            break;
        }
        let line = line.trim().to_string();
        if line == "salir" || line == "exit" {
            println!("adios");
            std::io::stdout().flush().ok();
            break;
        }
        println!("RECIBIDO: {line}");
        std::io::stdout().flush().ok();
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}
