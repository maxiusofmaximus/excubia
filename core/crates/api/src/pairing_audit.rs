//! The record of each `POST /v1/pair` decision. The API hands records to a sink
//! the host supplies, so this crate never touches the disk. A record carries no
//! code and no public key.

use hn_core_auth::unix_seconds;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PairOutcome {
    Paired,
    Recovered,
    Rejected,
    /// An admin device granted permissions to a pending one (`POST …/grant`).
    Granted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PairVia {
    Code,
    Recovery,
    Tailnet,
    Admin,
}

#[derive(Debug, Serialize)]
pub struct PairingAttempt {
    /// Unix seconds.
    pub ts: f64,
    pub outcome: PairOutcome,
    pub device_id: Option<String>,
    pub label: Option<String>,
    pub via: PairVia,
    pub source_ip: String,
    /// The granting device, on a `granted` line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub granted_by: Option<String>,
    /// The permissions granted, on a `granted` line.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<String>,
    /// The verification string shown to the Master. Not written to the audit file.
    #[serde(skip)]
    pub sas: Option<String>,
}

impl PairingAttempt {
    /// A record with no grant and no verification string.
    pub fn new(
        outcome: PairOutcome,
        via: PairVia,
        device_id: Option<&str>,
        label: Option<&str>,
        source_ip: &str,
    ) -> Self {
        Self {
            ts: unix_seconds(),
            outcome,
            device_id: device_id.map(str::to_string),
            label: label.map(str::to_string),
            via,
            source_ip: source_ip.to_string(),
            granted_by: None,
            permissions: Vec::new(),
            sas: None,
        }
    }
}

/// Where decisions go. Recording never fails the pairing: a sink that cannot
/// write reports it itself.
pub trait PairingAudit: Send + Sync + 'static {
    fn record(&self, attempt: &PairingAttempt);
}
