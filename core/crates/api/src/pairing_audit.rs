//! The record of each `POST /v1/pair` decision. The API hands records to a sink
//! the host supplies, so this crate never touches the disk. A record carries no
//! code and no public key.

use excubia_auth::unix_seconds;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PairOutcome {
    Paired,
    Recovered,
    Rejected,
    /// An admin device granted permissions to a pending one (`POST …/grant`).
    Granted,
    /// The PAKE code was spent by a `start` (`pairing.md` F2b). The explicit names matter: `lowercase` alone
    /// would write `pakestarted`.
    #[serde(rename = "pake_started")]
    PakeStarted,
    /// A PAKE attempt that did not end in a device: no live code, a bad message, no session, a wrong tag.
    #[serde(rename = "pake_failed")]
    PakeFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PairVia {
    Code,
    Recovery,
    Tailnet,
    Admin,
    /// The short PAKE code over TLS.
    Pake,
}

impl PairOutcome {
    /// The name the pairing audit writes, also used by the action log.
    pub fn name(self) -> &'static str {
        match self {
            Self::Paired => "paired",
            Self::Recovered => "recovered",
            Self::Rejected => "rejected",
            Self::Granted => "granted",
            Self::PakeStarted => "pake_started",
            Self::PakeFailed => "pake_failed",
        }
    }
}

impl PairVia {
    pub fn name(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Recovery => "recovery",
            Self::Tailnet => "tailnet",
            Self::Admin => "admin",
            Self::Pake => "pake",
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pake_events_have_their_own_stable_names() {
        let line = |outcome, via| {
            let attempt = PairingAttempt::new(outcome, via, None, None, "192.168.1.5");
            serde_json::to_value(&attempt).expect("json")
        };
        let started = line(PairOutcome::PakeStarted, PairVia::Pake);
        assert_eq!(
            (started["outcome"].as_str(), started["via"].as_str()),
            (Some("pake_started"), Some("pake"))
        );
        assert_eq!(
            line(PairOutcome::PakeFailed, PairVia::Pake)["outcome"],
            "pake_failed"
        );
        assert_eq!(
            line(PairOutcome::Paired, PairVia::Pake)["outcome"],
            "paired"
        );
        // The names that existed before did not move.
        assert_eq!(line(PairOutcome::Rejected, PairVia::Code)["via"], "code");
    }
}
