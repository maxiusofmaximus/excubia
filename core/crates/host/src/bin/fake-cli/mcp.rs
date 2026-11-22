use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

pub(super) struct Mcp {
    pub(super) child: Child,
    pub(super) stdin: ChildStdin,
    pub(super) stdout: BufReader<ChildStdout>,
    pub(super) next_id: u64,
}

impl Mcp {
    pub(super) fn start(config_path: &str) -> Result<Self, String> {
        let raw = std::fs::read_to_string(config_path)
            .map_err(|error| format!("read MCP config: {error}"))?;
        let config: Value =
            serde_json::from_str(&raw).map_err(|error| format!("parse MCP config: {error}"))?;
        let server = &config["mcp"];
        let command = server["command"]
            .as_str()
            .ok_or("MCP config has no command")?;
        let args = server["args"]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let env = server["env"].as_object().cloned().unwrap_or_default();
        let mut child = Command::new(command)
            .args(args)
            .envs(
                env.iter()
                    .filter_map(|(key, value)| value.as_str().map(|value| (key.as_str(), value))),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("spawn MCP server: {error}"))?;
        let stdin = child.stdin.take().ok_or("MCP stdin unavailable")?;
        let stdout = child.stdout.take().ok_or("MCP stdout unavailable")?;
        let mut mcp = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        };
        mcp.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "fake-cli", "version": "0.1.0" }
            }),
        )?;
        mcp.notify("notifications/initialized")?;
        Ok(mcp)
    }

    pub(super) fn notify(&mut self, method: &str) -> Result<(), String> {
        let message = json!({ "jsonrpc": "2.0", "method": method });
        writeln!(self.stdin, "{message}").map_err(|error| error.to_string())?;
        self.stdin.flush().map_err(|error| error.to_string())
    }

    pub(super) fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        writeln!(self.stdin, "{message}").map_err(|error| error.to_string())?;
        self.stdin.flush().map_err(|error| error.to_string())?;
        loop {
            let mut line = String::new();
            let read = self
                .stdout
                .read_line(&mut line)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                return Err("MCP server closed stdout".to_string());
            }
            if line.trim().is_empty() {
                continue;
            }
            let value: Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if value.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                return Err(format!("JSON-RPC error: {error}"));
            }
            return value
                .get("result")
                .cloned()
                .ok_or_else(|| "JSON-RPC response has no result".to_string());
        }
    }

    pub(super) fn call(&mut self, name: &str, arguments: Value) -> Result<Value, String> {
        let result = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )?;
        let is_error = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let text = result
            .get("content")
            .and_then(Value::as_array)
            .and_then(|content| content.first())
            .and_then(|content| content.get("text"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if is_error {
            return Err(text.to_string());
        }
        Ok(serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string())))
    }

    pub(super) fn reply(&mut self, text: &str) -> Result<(), String> {
        self.call("chat.reply", json!({ "text": text }))?;
        Ok(())
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
