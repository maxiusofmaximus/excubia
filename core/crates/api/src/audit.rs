//! The record of sensitive actions (`docs/contracts/audit.md`). The routes hand each action to a sink
//! the host supplies, so this crate never touches the disk. A sink never fails the request.

use std::collections::BTreeMap;

use hn_core_auth::unix_seconds;
use serde::Serialize;

/// One action the host took for a request. Values never carry codes, keys, links or message bodies.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    /// Unix seconds.
    pub ts: u64,
    /// The device that asked, or the one the action is about.
    pub device: Option<String>,
    pub action: &'static str,
    pub detail: BTreeMap<&'static str, String>,
    pub outcome: String,
}

impl AuditEvent {
    pub fn new(action: &'static str, device: Option<&str>, outcome: &str) -> Self {
        Self {
            ts: unix_seconds() as u64,
            device: device.map(str::to_string),
            action,
            detail: BTreeMap::new(),
            outcome: outcome.to_string(),
        }
    }

    pub fn with(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.detail.insert(key, value.into());
        self
    }
}

/// Where actions go. Recording never fails the request: a sink that cannot write reports it itself.
pub trait AuditSink: Send + Sync + 'static {
    fn record(&self, event: &AuditEvent);
}
