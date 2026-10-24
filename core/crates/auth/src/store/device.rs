//! The device model: public material only, plus the explicit grant set.

use serde::Serialize;

use crate::curve::Curve;

/// The five permissions of `docs/contracts/auth.md`. `Approve` is a reserved
/// identifier with no endpoint; unknown ids in a stored record are dropped
/// rather than guessed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Permission {
    ReadState,
    Respond,
    SendKey,
    Approve,
    Admin,
}

impl Permission {
    pub const ALL: [Permission; 5] = [
        Permission::ReadState,
        Permission::Respond,
        Permission::SendKey,
        Permission::Approve,
        Permission::Admin,
    ];

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|permission| permission.label() == label)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ReadState => "read_state",
            Self::Respond => "respond",
            Self::SendKey => "send_key",
            Self::Approve => "approve",
            Self::Admin => "admin",
        }
    }
}

/// A registered device. Public material only — the gateway never has a
/// private key.
#[derive(Clone, Debug)]
pub struct Device {
    pub device_id: String,
    pub curve: Curve,
    pub public_key: String,
    pub label: Option<String>,
    pub created_at: f64,
    pub last_seen_at: Option<f64>,
    pub revoked: bool,
    pub permissions: Vec<Permission>,
    pub fcm_token: Option<String>,
    pub owner_id: String,
    pub recovery_code_hash: Option<String>,
    /// In-memory bookkeeping for the 60 s `touch` rule; never published.
    pub persisted_at: Option<f64>,
}

impl Device {
    /// What `GET /v1/devices` returns: never a public key, never an FCM token.
    pub fn to_public(&self) -> PublicDevice {
        let mut permissions: Vec<String> = self
            .permissions
            .iter()
            .map(|permission| permission.label().to_string())
            .collect();
        permissions.sort();
        PublicDevice {
            device_id: self.device_id.clone(),
            curve: self.curve.label().to_string(),
            label: self.label.clone(),
            owner_id: self.owner_id.clone(),
            created_at: self.created_at,
            last_seen_at: self.last_seen_at,
            revoked: self.revoked,
            permissions,
        }
    }

    pub fn has(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PublicDevice {
    pub device_id: String,
    pub curve: String,
    pub label: Option<String>,
    pub owner_id: String,
    pub created_at: f64,
    pub last_seen_at: Option<f64>,
    pub revoked: bool,
    pub permissions: Vec<String>,
}
