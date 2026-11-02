//! The record of each `POST /v1/pair` decision. The API hands records to a sink
//! the host supplies, so this crate never touches the disk. A record carries no
//! code and no public key.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PairOutcome {
    Paired,
    Recovered,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PairVia {
    Code,
    Recovery,
    Tailnet,
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
}

/// Where decisions go. Recording never fails the pairing: a sink that cannot
/// write reports it itself.
pub trait PairingAudit: Send + Sync + 'static {
    fn record(&self, attempt: &PairingAttempt);
}
