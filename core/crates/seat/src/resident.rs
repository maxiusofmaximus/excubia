//! `excubia-mcp resident -- <cli> <args…>`: a project seat (lead or specialist) for a
//! CLI that has no resident mode of its own. It lives in the seat's panel, inside the
//! project folder, and reads one task per line from the host (`{"delegation_id",
//! "from", "task"}`). Each task runs one `<cli> <args…>` with the task on stdin; what
//! the CLI prints is shown in the panel. The agent may close the task itself with
//! `delegation.complete`; if it does not, its last words close it here, so a task never
//! hangs because the model forgot the tool.

use std::process::Stdio;

use excubia_chat::ToolRequest;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::env;
use crate::mcp::post_tool;

/// The most of the CLI's output kept as the result: the end, where the answer is.
const RESULT_CHARS: usize = 8000;

pub async fn run_resident(engine: &[String]) -> Result<(), String> {
    let (program, args) = engine
        .split_first()
        .ok_or("resident needs the CLI after --")?;
    let url = env::var_or_legacy("EXCUBIA_TOOL_URL", "HN_TOOL_URL")
        .ok_or("EXCUBIA_TOOL_URL is not set; the resident only runs under the host")?;
    let token = env::var_or_legacy("EXCUBIA_TOOL_TOKEN", "HN_TOOL_TOKEN")
        .ok_or("EXCUBIA_TOOL_TOKEN is not set; the resident only runs under the host")?;
    println!("Listo. Espero tareas.");
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|error| format!("read task: {error}"))?
    {
        let Some(task) = parse_task(&line) else {
            continue;
        };
        let Task::Work { id, from, text } = task else {
            break;
        };
        println!("\n── Tarea de {from} ──");
        serve_task(program, args, &url, &token, &id, &from, &text).await;
        println!("── Tarea cerrada ──");
    }
    Ok(())
}

/// Runs one task through the CLI and closes it with `delegation.complete`. The CLI gets
/// the task on stdin; the token travels only in the MCP call, never in the prompt.
async fn serve_task(
    program: &str,
    args: &[String],
    url: &str,
    token: &str,
    id: &str,
    from: &str,
    text: &str,
) {
    let before = changes().await;
    let (ran, answer) = match run_engine(program, args, &task_prompt(from, text)).await {
        Ok((true, output)) if !output.trim().is_empty() => (true, output),
        Ok((true, _)) => (true, "Terminé la tarea sin texto de resultado.".to_string()),
        Ok((false, output)) => (false, format!("No pude terminar la tarea. Última salida:\n{output}")),
        Err(error) => (false, format!("No pude arrancar el agente: {error}")),
    };
    let (declared_failed, mut result) = closing_status(&answer);
    let failed = !ran || declared_failed;
    result.push_str(&changed_files(&before, &changes().await));
    let request = ToolRequest {
        tool: "delegation.complete".to_string(),
        arguments: json!({
            "delegation_id": id,
            "result": result,
            "outcome": if failed { "failed" } else { "done" },
        }),
    };
    // Refused when the agent already closed the task itself: that is the good case.
    if let Err(error) = post_tool(url, token, &request).await {
        if error.code != "delegation_cancelled" {
            eprintln!("delegation.complete: {} ({})", error.message, error.code);
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Task {
    Work {
        id: String,
        from: String,
        text: String,
    },
    Cancel,
}

fn parse_task(line: &str) -> Option<Task> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("cancel").is_some() {
        return Some(Task::Cancel);
    }
    let id = value["delegation_id"].as_str()?.to_string();
    Some(Task::Work {
        id,
        from: value["from"].as_str().unwrap_or("Alfred").to_string(),
        text: value["task"].as_str().unwrap_or_default().to_string(),
    })
}

fn task_prompt(from: &str, text: &str) -> String {
    format!(
        "Tarea de {from}. Trabaja en la carpeta actual, que es la del proyecto.\n\n\
         {text}\n\n\
         Tu respuesta final es el resultado de la tarea: escríbelo completo, en español. \
         Como última línea, escribe exactamente `ESTADO: HECHO` si la completaste o \
         `ESTADO: FALLIDO` si no pudiste. La tarea se cierra sola con esa respuesta."
    )
}

/// The status line the task asks for as the answer's last line: whether it says the work
/// failed, and the answer without that line. No status line means done, as before.
fn closing_status(answer: &str) -> (bool, String) {
    let mut lines: Vec<&str> = answer.lines().collect();
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    let failed = match lines.last().map(|line| line.trim()) {
        Some("ESTADO: FALLIDO") => true,
        Some("ESTADO: HECHO") => false,
        _ => return (false, answer.to_string()),
    };
    lines.pop();
    (failed, lines.join("\n").trim_end().to_string())
}

/// Each changed file of the project folder with its lines added and removed against the
/// last commit; a new file counts all its lines as added. Empty outside a git repository.
type Changes = std::collections::BTreeMap<String, (u64, u64)>;

async fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(["-c", "core.quotepath=false"])
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).to_string())
}

async fn changes() -> Changes {
    let mut changes = Changes::new();
    let tracked = match git(&["diff", "--numstat", "HEAD"]).await {
        Some(text) => text,
        None => git(&["diff", "--numstat"]).await.unwrap_or_default(),
    };
    for line in tracked.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        changes.insert(
            path.to_string(),
            (added.parse().unwrap_or(0), removed.parse().unwrap_or(0)),
        );
    }
    let untracked = git(&["ls-files", "--others", "--exclude-standard"])
        .await
        .unwrap_or_default();
    for path in untracked.lines().filter(|path| !path.is_empty()) {
        let lines = std::fs::read_to_string(path)
            .map(|text| text.lines().count() as u64)
            .unwrap_or(0);
        changes.insert(path.to_string(), (lines, 0));
    }
    changes
}

/// The files this task touched, as a short list under the result.
fn changed_files(before: &Changes, after: &Changes) -> String {
    const MAX: usize = 20;
    let touched: Vec<String> = after
        .iter()
        .filter(|(path, counts)| before.get(*path) != Some(counts))
        .map(|(path, &(added, removed))| {
            let counts = match (added, removed) {
                (added, 0) => format!("{added} líneas añadidas"),
                (0, removed) => format!("{removed} líneas quitadas"),
                (added, removed) => format!("{added} añadidas, {removed} quitadas"),
            };
            format!("- {path}: {counts}")
        })
        .collect();
    if touched.is_empty() {
        return String::new();
    }
    let more = touched.len().saturating_sub(MAX);
    let mut list = touched.into_iter().take(MAX).collect::<Vec<_>>().join("\n");
    if more > 0 {
        list.push_str(&format!("\n- y {more} más"));
    }
    format!("\n\nArchivos cambiados:\n{list}")
}

/// Runs the CLI once: the prompt on stdin, both outputs shown live, the end of stdout kept.
async fn run_engine(
    program: &str,
    args: &[String],
    prompt: &str,
) -> Result<(bool, String), String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| error.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let bytes = prompt.as_bytes().to_vec();
        tokio::spawn(async move {
            let _ = stdin.write_all(&bytes).await;
        });
    }
    let out = child.stdout.take().ok_or("no stdout")?;
    let err = child.stderr.take().ok_or("no stderr")?;
    let (kept, _, status) = tokio::join!(
        echo(out, tokio::io::stdout(), true),
        echo(err, tokio::io::stderr(), false),
        child.wait()
    );
    let status = status.map_err(|error| error.to_string())?;
    Ok((status.success(), tail(&kept, RESULT_CHARS)))
}

async fn echo(
    mut from: impl AsyncRead + Unpin,
    mut to: impl AsyncWrite + Unpin,
    keep: bool,
) -> String {
    let mut kept = Vec::new();
    let mut buffer = [0u8; 4096];
    while let Ok(read) = from.read(&mut buffer).await {
        if read == 0 {
            break;
        }
        let _ = to.write_all(&buffer[..read]).await;
        let _ = to.flush().await;
        if keep {
            kept.extend_from_slice(&buffer[..read]);
        }
    }
    String::from_utf8_lossy(&kept).to_string()
}

fn tail(text: &str, chars: usize) -> String {
    let count = text.chars().count();
    let text = text.trim();
    if count <= chars {
        return text.to_string();
    }
    let skipped: String = text.chars().skip(count.saturating_sub(chars)).collect();
    format!("…{skipped}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_line_is_work_a_cancel_stops_and_noise_is_skipped() {
        assert_eq!(
            parse_task(r#"{"delegation_id":"dlg_1","from":"Alfred","task":"Lee README.md"}"#),
            Some(Task::Work {
                id: "dlg_1".to_string(),
                from: "Alfred".to_string(),
                text: "Lee README.md".to_string()
            })
        );
        assert_eq!(parse_task(r#"{"cancel":true}"#), Some(Task::Cancel));
        assert_eq!(parse_task("   "), None);
        assert_eq!(parse_task("not json"), None);
        assert_eq!(parse_task(r#"{"task":"sin id"}"#), None);
    }

    #[test]
    fn the_result_keeps_the_end_of_a_long_output() {
        let long = format!("{}FIN", "x".repeat(9000));
        let kept = tail(&long, 100);
        assert!(kept.ends_with("FIN"));
        assert!(kept.chars().count() <= 101);
        assert_eq!(tail("  corto \n", 100), "corto");
    }

    #[test]
    fn the_prompt_carries_the_task_and_says_the_answer_closes_it() {
        let prompt = task_prompt("Alfred", "Haz X");
        assert!(prompt.contains("Haz X") && prompt.contains("respuesta final"));
    }

    #[test]
    fn only_files_this_task_changed_are_listed() {
        let before: Changes = [("viejo.rs".to_string(), (3, 1))].into_iter().collect();
        let after: Changes = [
            ("viejo.rs".to_string(), (3, 1)),
            ("nuevo.txt".to_string(), (2, 0)),
            ("src/app.rs".to_string(), (5, 2)),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            changed_files(&before, &after),
            "\n\nArchivos cambiados:\n- nuevo.txt: 2 líneas añadidas\n- src/app.rs: 5 añadidas, 2 quitadas"
        );
        assert_eq!(changed_files(&after, &after), "");
    }

    #[test]
    fn the_last_status_line_says_how_the_task_ended_and_leaves_the_answer() {
        assert_eq!(
            closing_status("Lo hice.\n\nESTADO: HECHO\n"),
            (false, "Lo hice.".to_string())
        );
        assert_eq!(
            closing_status("No pude escribir el archivo.\nESTADO: FALLIDO"),
            (true, "No pude escribir el archivo.".to_string())
        );
        // No status line: done, and the answer is untouched.
        assert_eq!(
            closing_status("Resultado: no completado."),
            (false, "Resultado: no completado.".to_string())
        );
        // A status word in the middle of the answer is not a status.
        assert_eq!(
            closing_status("ESTADO: FALLIDO\nluego seguí y acabé"),
            (false, "ESTADO: FALLIDO\nluego seguí y acabé".to_string())
        );
    }

    /// A stub CLI that answers the task and states its end: the runner closes the
    /// delegation through MCP with the panel's token in the header, never in the prompt.
    #[cfg(windows)]
    fn stub_cli(status: &str) -> (String, Vec<String>) {
        (
            "cmd".to_string(),
            vec!["/C".to_string(), format!("echo Resultado listo & echo {status}")],
        )
    }

    #[cfg(not(windows))]
    fn stub_cli(status: &str) -> (String, Vec<String>) {
        (
            "sh".to_string(),
            vec!["-c".to_string(), format!("echo Resultado listo; echo '{status}'")],
        )
    }

    #[tokio::test]
    async fn a_stub_cli_task_is_closed_through_mcp_with_the_result() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock");
        let address = listener.local_addr().expect("mock address");
        let server = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buffer = vec![0_u8; 16384];
            let read = socket.read(&mut buffer).await.unwrap_or(0);
            let body = br#"{"ok":true,"result":{}}"#;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(body).await;
            String::from_utf8_lossy(&buffer[..read]).to_string()
        });
        let (program, args) = stub_cli("ESTADO: HECHO");
        let url = format!("http://{address}/internal/chat/tools");
        serve_task(&program, &args, &url, "tok-panel", "dlg_1", "Alfred", "Haz X").await;
        let request = server.await.expect("server");
        assert!(request.contains("delegation.complete"), "{request}");
        assert!(request.contains("\"delegation_id\":\"dlg_1\""), "{request}");
        assert!(request.contains("\"outcome\":\"done\""), "{request}");
        assert!(request.contains("Resultado listo"), "{request}");
        assert!(request.to_lowercase().contains("x-excubia-tool-token: tok-panel"), "{request}");
    }
}
