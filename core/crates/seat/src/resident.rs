//! `excubia-mcp resident -- <cli> <args…>`: a project seat (lead or specialist) for a
//! CLI that has no resident mode of its own. It lives in the seat's panel, inside the
//! project folder, and reads one task per line from the host (`{"delegation_id",
//! "from", "task"}`). Each task runs one `<cli> <args…>` with the task on stdin; what
//! the CLI prints is shown in the panel. The agent may close the task itself with
//! `delegation.complete`; if it does not, its last words close it here, so a task never
//! hangs because the model forgot the tool.

use std::future::Future;
use std::process::Stdio;
use std::time::Instant;

use excubia_chat::ToolRequest;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::env;
use crate::free_models::{is_free_limit, FreeModels, FREE_MODELS};
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
    // The free models this panel found spent; it lives as long as the panel, so the memory does too.
    let book = FreeModels::default();
    println!("Listo. Espero tareas.");
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|error| format!("read task: {error}"))?
    {
        let Some(Task { id, from, text }) = parse_task(&line) else {
            continue;
        };
        println!("\n── Tarea de {from} ──");
        serve_task(program, args, &book, &url, &token, &id, &from, &text).await;
        println!("── Tarea cerrada ──");
    }
    Ok(())
}

/// Runs one task through the CLI and closes it with `delegation.complete`. The CLI gets
/// the task on stdin; the token travels only in the MCP call, never in the prompt.
async fn serve_task(
    program: &str,
    args: &[String],
    book: &FreeModels,
    url: &str,
    token: &str,
    id: &str,
    from: &str,
    text: &str,
) {
    let before = changes().await;
    let prompt = task_prompt(from, text);
    let (program, prompt) = (program, prompt.as_str());
    let outcome = run_task(book, args, |run_args| async move {
        run_engine(program, &run_args, prompt).await
    })
    .await;
    let (ran, answer) = match outcome {
        Ok((true, output, _)) if !output.trim().is_empty() => (true, output),
        Ok((true, _, _)) => (true, "Terminé la tarea sin texto de resultado.".to_string()),
        Ok((false, output, errors)) => {
            let shown = if output.trim().is_empty() { tail(&errors, RESULT_CHARS) } else { output };
            (false, format!("No pude terminar la tarea. Última salida:\n{shown}"))
        }
        Err(error) => (false, format!("No pude arrancar el agente: {error}")),
    };
    // The CLI inherits the tool token; a CLI that dumps its env must not carry it into the chat.
    let answer = if token.is_empty() { answer } else { answer.replace(token, "***") };
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

/// Runs one task's CLI on the free OpenCode models in order, within the same task. A run whose
/// free tier is spent marks its model and the next one is tried; a run that ends without any
/// text tries the next one unmarked; the first run that answers ends the task. Any other CLI
/// runs once. Returns the run's stdout and stderr.
async fn run_task<Fut>(
    book: &FreeModels,
    args: &[String],
    mut run: impl FnMut(Vec<String>) -> Fut,
) -> Result<(bool, String, String), String>
where
    Fut: Future<Output = Result<(bool, String, String, bool), String>>,
{
    let Some(slot) = free_model_slot(args) else {
        let (ran, output, errors, _) = run(args.to_vec()).await?;
        return Ok((ran, output, errors));
    };
    let mut last = None;
    for model in book.usable(Instant::now()) {
        let mut run_args = args.to_vec();
        run_args[slot] = model.to_string();
        let (ran, output, errors, limited) = run(run_args).await?;
        if limited {
            book.mark_exhausted(model, Instant::now());
        }
        // A run that said it succeeded keeps its answer even if its text quotes the limit.
        if ran || (!limited && !output.trim().is_empty()) {
            return Ok((ran, output, errors));
        }
        if !limited {
            last = Some((ran, output, errors));
        }
    }
    last.ok_or_else(|| "todos los modelos gratuitos están agotados de momento".to_string())
}

/// Where the free OpenCode model sits in a resident's CLI args: the value after `--model`, when
/// it is one of `FREE_MODELS`. Any other CLI or model keeps its args as they are.
fn free_model_slot(args: &[String]) -> Option<usize> {
    let slot = args.iter().position(|arg| arg == "--model")? + 1;
    let model = args.get(slot)?;
    FREE_MODELS.iter().any(|free| free == model).then_some(slot)
}

#[derive(Debug, PartialEq, Eq)]
struct Task {
    id: String,
    from: String,
    text: String,
}

/// A line that is not a task is skipped. A cancel is one of those: the host sends it when a
/// delegation is cancelled, and the panel stays up for the next task.
/// shortcut: a task queued behind a cancelled one still runs, since a running CLI cannot be
/// interrupted from here; upgrade when the CLI offers a cancel.
fn parse_task(line: &str) -> Option<Task> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("cancel").is_some() {
        return None;
    }
    Some(Task {
        id: value["delegation_id"].as_str()?.to_string(),
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

/// Runs the CLI once: the prompt on stdin, both outputs shown live, the end of each output kept.
/// The last value says whether the output reports the free tier of the model spent.
async fn run_engine(
    program: &str,
    args: &[String],
    prompt: &str,
) -> Result<(bool, String, String, bool), String> {
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
    let (kept, errors, status) = tokio::join!(
        echo(out, tokio::io::stdout()),
        echo(err, tokio::io::stderr()),
        child.wait()
    );
    let status = status.map_err(|error| error.to_string())?;
    let limited = is_free_limit(&format!("{kept}\n{errors}"));
    Ok((status.success(), tail(&kept, RESULT_CHARS), tail(&errors, RESULT_CHARS), limited))
}

async fn echo(mut from: impl AsyncRead + Unpin, mut to: impl AsyncWrite + Unpin) -> String {
    let mut kept = Vec::new();
    let mut buffer = [0u8; 4096];
    while let Ok(read) = from.read(&mut buffer).await {
        if read == 0 {
            break;
        }
        let _ = to.write_all(&buffer[..read]).await;
        let _ = to.flush().await;
        kept.extend_from_slice(&buffer[..read]);
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
    fn a_task_line_is_work_and_a_cancel_or_noise_is_skipped() {
        assert_eq!(
            parse_task(r#"{"delegation_id":"dlg_1","from":"Alfred","task":"Lee README.md"}"#),
            Some(Task {
                id: "dlg_1".to_string(),
                from: "Alfred".to_string(),
                text: "Lee README.md".to_string()
            })
        );
        // A cancel does not end the panel: the next task is still read.
        assert_eq!(parse_task(r#"{"cancel":"dlg_1","reason":"user_cancelled"}"#), None);
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

    fn opencode_args(model: &str) -> Vec<String> {
        ["run", "--standalone", "--model", model, "--agent", "alfred"].map(String::from).to_vec()
    }

    #[test]
    fn only_a_free_model_in_the_model_flag_is_rotated() {
        assert_eq!(free_model_slot(&opencode_args(FREE_MODELS[0])), Some(3));
        assert_eq!(free_model_slot(&opencode_args("openai/gpt-5")), None);
        assert_eq!(free_model_slot(&["exec".to_string()]), None);
    }

    /// The first free model is spent: its run marks it and the task goes on, in the same run, to
    /// the second, which answers. The spent model is skipped by the tasks after it.
    #[tokio::test]
    async fn a_task_goes_on_to_the_next_free_model_when_the_first_is_spent() {
        let book = FreeModels::default();
        let args = opencode_args(FREE_MODELS[0]);
        let mut ran_on = Vec::new();
        let outcome = run_task(&book, &args, |run_args| {
            ran_on.push(run_args[3].clone());
            let spent = run_args[3] == FREE_MODELS[0];
            async move {
                Ok(if spent {
                    (false, "Free usage limit reached".to_string(), String::new(), true)
                } else {
                    (true, "Listo".to_string(), String::new(), false)
                })
            }
        })
        .await;
        assert_eq!(outcome, Ok((true, "Listo".to_string(), String::new())));
        assert_eq!(ran_on, [FREE_MODELS[0], FREE_MODELS[1]]);
        assert!(!book.usable(Instant::now()).contains(&FREE_MODELS[0]));
    }

    /// Every free model spent: the task says so in Spanish, and with none left nothing runs.
    #[tokio::test]
    async fn when_every_free_model_is_spent_the_task_says_so() {
        let book = FreeModels::default();
        let args = opencode_args(FREE_MODELS[0]);
        let mut runs = 0;
        let outcome = run_task(&book, &args, |_| {
            runs += 1;
            async { Ok((false, "Free usage limit reached".to_string(), String::new(), true)) }
        })
        .await;
        assert_eq!(runs, FREE_MODELS.len());
        assert_eq!(
            outcome,
            Err("todos los modelos gratuitos están agotados de momento".to_string())
        );

        let again = run_task(&book, &args, |_| async {
            Ok((true, String::new(), String::new(), false))
        })
        .await;
        assert!(again.is_err(), "{again:?}");
    }

    /// A run that ends without text moves on to the next model without marking the first one.
    #[tokio::test]
    async fn a_silent_failure_tries_the_next_model_unmarked() {
        let book = FreeModels::default();
        let args = opencode_args(FREE_MODELS[0]);
        let outcome = run_task(&book, &args, |run_args| {
            let answers = run_args[3] == FREE_MODELS[1];
            async move { Ok((answers, String::new(), "boom".to_string(), false)) }
        })
        .await;
        assert_eq!(outcome, Ok((true, String::new(), "boom".to_string())));
        assert!(book.usable(Instant::now()).contains(&FREE_MODELS[0]));
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

    /// A local MCP endpoint that answers one call; the URL to post to, and the request it got.
    async fn mock_tool_server() -> (String, tokio::task::JoinHandle<String>) {
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
        (format!("http://{address}/internal/chat/tools"), server)
    }

    #[tokio::test]
    async fn a_stub_cli_task_is_closed_through_mcp_with_the_result() {
        let (url, server) = mock_tool_server().await;
        let (program, args) = stub_cli("ESTADO: HECHO");
        let book = FreeModels::default();
        serve_task(&program, &args, &book, &url, "tok-panel", "dlg_1", "Alfred", "Haz X").await;
        let request = server.await.expect("server");
        assert!(request.contains("delegation.complete"), "{request}");
        assert!(request.contains("\"delegation_id\":\"dlg_1\""), "{request}");
        assert!(request.contains("\"outcome\":\"done\""), "{request}");
        assert!(request.contains("Resultado listo"), "{request}");
        assert!(request.to_lowercase().contains("x-excubia-tool-token: tok-panel"), "{request}");
    }

    /// A CLI that fails with nothing on stdout: the task is closed as failed and the result shows
    /// the end of stderr, where the reason is.
    #[cfg(windows)]
    fn failing_cli() -> (String, Vec<String>) {
        ("cmd".to_string(), vec!["/C".to_string(), "echo boom 1>&2 & exit /b 3".to_string()])
    }

    #[cfg(not(windows))]
    fn failing_cli() -> (String, Vec<String>) {
        ("sh".to_string(), vec!["-c".to_string(), "echo boom >&2; exit 3".to_string()])
    }

    #[tokio::test]
    async fn a_failure_with_empty_stdout_shows_the_end_of_stderr() {
        let (url, server) = mock_tool_server().await;
        let (program, args) = failing_cli();
        let book = FreeModels::default();
        serve_task(&program, &args, &book, &url, "tok-panel", "dlg_2", "Alfred", "Haz X").await;
        let request = server.await.expect("server");
        assert!(request.contains("\"outcome\":\"failed\""), "{request}");
        assert!(request.contains("Última salida:\\nboom"), "{request}");
    }
}
