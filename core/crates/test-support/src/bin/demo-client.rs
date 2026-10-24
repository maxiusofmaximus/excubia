//! `demo-client` — a stand-in phone for the manual demo: pairs through
//! `POST /v1/pair`, keeps its identity in a file of its own (like a phone's
//! keystore), and drives the signed API with the same code the integration
//! tests use.
//!
//! Every command prints `HTTP <status>` followed by the body and exits
//! non-zero on a non-2xx answer, so a script can assert the refusals.

use std::process::ExitCode;

use hn_core_test_support::{pair_device, DemoIdentity, HttpError, SignedClient};
use serde::{Deserialize, Serialize};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("demo-client: {message}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Serialize, Deserialize)]
struct StoredIdentity {
    device_id: String,
    label: String,
    private_key: String,
}

async fn run(args: &[String]) -> Result<(), String> {
    let Some(command) = args.first() else {
        return Err(USAGE.to_string());
    };
    let flags = Flags::parse(&args[1..])?;
    match command.as_str() {
        "pair" => pair(&flags).await,
        "state" => print(load_client(&flags)?.state().await),
        "catalog" => print(load_client(&flags)?.catalog().await),
        "create" => {
            let agent = flags.require("--agent")?;
            let cwd = flags.require("--cwd")?;
            let label = flags.value("--label").unwrap_or_default();
            print(
                load_client(&flags)?
                    .create_workspace(&agent, &cwd, &label)
                    .await,
            )
        }
        "events" => events(&flags).await,
        "respond" => {
            let pane = flags.require("--pane")?;
            let text = flags.require("--text")?;
            print(load_client(&flags)?.respond(&pane, &text).await)
        }
        "read" => {
            let pane = flags.require("--pane")?;
            let lines = flags
                .value("--lines")
                .and_then(|value| value.parse().ok())
                .unwrap_or(60);
            print(load_client(&flags)?.read(&pane, lines).await)
        }
        "keys" => {
            let pane = flags.require("--pane")?;
            let keys: Vec<&str> = flags.all("--key");
            if keys.is_empty() {
                return Err("keys needs at least one --key".to_string());
            }
            let confirm = flags.has("--confirm");
            print(load_client(&flags)?.keys(&pane, &keys, confirm).await)
        }
        "stop" => {
            let pane = flags.require("--pane")?;
            print(load_client(&flags)?.stop(&pane).await)
        }
        "chat-bootstrap" => print(load_client(&flags)?.chat_bootstrap().await),
        "chat-create" => {
            let title = flags.require("--title")?;
            print(load_client(&flags)?.chat_create(&title, None).await)
        }
        "chat-send" => {
            let conversation = flags.require("--conversation")?;
            let client_id = flags.require("--client-id")?;
            let text = flags.require("--text")?;
            print(
                load_client(&flags)?
                    .chat_send(&conversation, &client_id, &text)
                    .await,
            )
        }
        "approvals" => {
            let status = flags.value("--status");
            let params: Vec<(&str, &str)> = status.iter().map(|s| ("status", s.as_str())).collect();
            print(load_client(&flags)?.chat_approvals(&params).await)
        }
        "decide" => {
            let approval = flags.require("--approval")?;
            let decision = flags.require("--decision")?;
            let client_id = flags.require("--client-id")?;
            print(
                load_client(&flags)?
                    .chat_decide_approval(&approval, &decision, &client_id)
                    .await,
            )
        }
        "approval-mode" => print(load_client(&flags)?.chat_approval_settings().await),
        "set-approval-mode" => {
            let mode = flags.require("--mode")?;
            let client_id = flags.require("--client-id")?;
            let project = flags.value("--project");
            print(
                load_client(&flags)?
                    .chat_set_approval_mode(&client_id, &mode, project.as_deref())
                    .await,
            )
        }
        "devices" => print(load_client(&flags)?.devices().await),
        "revoke" => {
            let device = flags.require("--device")?;
            print(load_client(&flags)?.revoke(&device).await)
        }
        other => Err(format!("unknown command: {other}\n\n{USAGE}")),
    }
}

async fn pair(flags: &Flags) -> Result<(), String> {
    let url = flags.require("--url")?;
    let code = flags.require("--code")?;
    let label = flags
        .value("--label")
        .unwrap_or_else(|| "demo phone".to_string());
    let identity_path = flags.require("--identity")?;
    let identity = DemoIdentity::generate("", &label);
    let response = pair_device(&url, "p256", &identity.public_key_b64(), &label, &code)
        .await
        .map_err(|error| error.to_string())?;
    let stored = StoredIdentity {
        device_id: response.device_id.clone(),
        label,
        private_key: identity.private_key_b64(),
    };
    let json = serde_json::to_string_pretty(&stored).map_err(|error| error.to_string())?;
    std::fs::write(&identity_path, json).map_err(|error| error.to_string())?;
    println!("demo-client pair: HTTP 200");
    println!(
        "{}",
        serde_json::json!({
            "device_id": response.device_id,
            "permissions": response.permissions,
            "recovery_code": response.recovery_code,
            "owner_id": response.owner_id,
            "identity_file": identity_path,
        })
    );
    Ok(())
}

async fn events(flags: &Flags) -> Result<(), String> {
    let seconds: u64 = flags
        .value("--seconds")
        .and_then(|value| value.parse().ok())
        .unwrap_or(5);
    let client = load_client(flags)?;
    match client.events(seconds).await {
        Ok(raw) => {
            println!("demo-client events: HTTP 200 ({seconds}s capture)");
            print!("{raw}");
            Ok(())
        }
        Err(error) => fail(&error),
    }
}

fn load_client(flags: &Flags) -> Result<SignedClient, String> {
    let url = flags.require("--url")?;
    let identity_path = flags.require("--identity")?;
    let raw = std::fs::read_to_string(&identity_path)
        .map_err(|error| format!("identity {identity_path}: {error}"))?;
    let stored: StoredIdentity =
        serde_json::from_str(&raw).map_err(|error| format!("identity {identity_path}: {error}"))?;
    let identity =
        DemoIdentity::from_private_key(&stored.device_id, &stored.label, &stored.private_key)
            .ok_or("identity file has an unreadable private key")?;
    Ok(SignedClient::new(&url, identity))
}

fn print(outcome: Result<serde_json::Value, HttpError>) -> Result<(), String> {
    match outcome {
        Ok(value) => {
            println!("demo-client: HTTP 200");
            println!(
                "{}",
                serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?
            );
            Ok(())
        }
        Err(error) => fail(&error),
    }
}

fn fail(error: &HttpError) -> Result<(), String> {
    println!("demo-client: HTTP {}", error.status);
    println!("{}", error.body);
    Err(format!("HTTP {}", error.status))
}

struct Flags {
    entries: Vec<(String, String)>,
}

impl Flags {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut entries = Vec::new();
        let mut index = 0;
        while index < args.len() {
            let flag = &args[index];
            if !flag.starts_with("--") {
                return Err(format!("unexpected argument: {flag}"));
            }
            if flag == "--confirm" {
                entries.push((flag.clone(), "true".to_string()));
                index += 1;
                continue;
            }
            let value = args
                .get(index + 1)
                .ok_or_else(|| format!("{flag} needs a value"))?;
            entries.push((flag.clone(), value.clone()));
            index += 2;
        }
        Ok(Self { entries })
    }

    fn value(&self, name: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|(flag, _)| flag == name)
            .map(|(_, value)| value.clone())
    }

    fn all(&self, name: &str) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|(flag, _)| flag == name)
            .map(|(_, value)| value.as_str())
            .collect()
    }

    fn has(&self, name: &str) -> bool {
        self.entries.iter().any(|(flag, _)| flag == name)
    }

    fn require(&self, name: &str) -> Result<String, String> {
        self.value(name)
            .ok_or_else(|| format!("{name} is required"))
    }
}

const USAGE: &str = "demo-client <pair|state|events|respond|read|keys|devices|revoke|\
chat-bootstrap|chat-create|chat-send|approvals|decide|approval-mode|set-approval-mode> \
--url URL --identity FILE ...";
