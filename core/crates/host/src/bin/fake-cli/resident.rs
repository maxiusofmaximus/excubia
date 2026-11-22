use std::io::{BufRead, BufReader};
use std::time::Duration;

use serde_json::{json, Value};

use super::mcp::Mcp;
use super::scenario::{first_line, strip_markers};
use super::uac::{run_uac_hook, uac_ttl};

/// A resident project seat: read a task, answer it, read the next one.
pub(super) fn resident(config_path: &str) -> Result<(), String> {
    let role = std::env::var("EXCUBIA_SEAT_ROLE").unwrap_or_else(|_| "specialist".to_string());
    let mut mcp = Mcp::start(config_path)?;
    let stdin = std::io::stdin();
    for line in BufReader::new(stdin.lock()).lines() {
        let line = line.map_err(|error| format!("read task: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let task: Value = match serde_json::from_str(&line) {
            Ok(task) => task,
            Err(error) => {
                eprintln!("fake-cli: unreadable task: {error}");
                continue;
            }
        };
        if task.get("cancel").is_some() {
            eprintln!("fake-cli: task cancelled");
            break;
        }
        let delegation_id = task["delegation_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let text = task["task"].as_str().unwrap_or_default().to_string();
        if delegation_id.is_empty() {
            continue;
        }
        handle_task(&mut mcp, &role, &delegation_id, &text)?;
    }
    Ok(())
}

/// What one resident seat does with the task it was given.
fn handle_task(mcp: &mut Mcp, role: &str, delegation_id: &str, text: &str) -> Result<(), String> {
    if text.contains("[[fake:hang]]") {
        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    }
    if text.contains("[[fake:silent]]") {
        eprintln!("fake-cli: task {delegation_id} left pending");
        return Ok(());
    }
    if text.contains("[[fake:uac]]") || text.contains("[[fake:uac-ttl=") {
        let outcome = run_uac_hook(text, uac_ttl(text))?;
        eprintln!("fake-cli: the UAC hook answered {outcome}");
        mcp.call(
            "delegation.complete",
            json!({
                "delegation_id": delegation_id,
                "result": format!("uac {outcome}")
            }),
        )?;
        return Ok(());
    }
    if role == "lead" && text.contains("[[fake:lead-spawn]]") {
        let spawned = mcp.call(
            "agent.spawn",
            json!({ "name": "Ana", "role": "specialist" }),
        )?;
        let specialist = spawned["seat"]["agent_id"]
            .as_str()
            .ok_or("agent.spawn returned no seat")?
            .to_string();
        let child = mcp.call(
            "delegation.assign",
            json!({
                "to_agent_id": specialist,
                "task": format!("[[fake:complete]] {}", strip_markers(text))
            }),
        )?;
        let child_id = child["delegation_id"].as_str().unwrap_or("?").to_string();
        mcp.call(
            "delegation.complete",
            json!({
                "delegation_id": delegation_id,
                "result": format!("Lead: trabajo repartido en {child_id}")
            }),
        )?;
        return Ok(());
    }
    mcp.call(
        "delegation.complete",
        json!({
            "delegation_id": delegation_id,
            "result": format!("{role}: {}", first_line(text))
        }),
    )?;
    Ok(())
}
