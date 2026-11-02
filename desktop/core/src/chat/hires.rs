//! The team: the role catalogue, the professionals hired on this host and the
//! first-run onboarding. Contracting only registers a hire; every write needs
//! the device's `APPROVE`, and a device without it gets `403` from the host.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::error::ChatResult;
use super::validate_prefixed_id;
use crate::client::HostClient;

/// The roles each onboarding level preselects in an area.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LevelPresets {
    #[serde(default)]
    pub personal: Vec<String>,
    #[serde(default)]
    pub profesional: Vec<String>,
    #[serde(default)]
    pub equipo: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RoleArea {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub levels: LevelPresets,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Role {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub area: String,
    /// A character id, or `None` for the initial.
    #[serde(default)]
    pub avatar: Option<String>,
}

/// The professions a person can hire. Read-only: the same on every read.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RoleCatalog {
    #[serde(default)]
    pub areas: Vec<RoleArea>,
    #[serde(default)]
    pub roles: Vec<Role>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Hire {
    pub id: String,
    pub role: String,
    pub area: String,
    pub name: String,
    #[serde(default)]
    pub avatar: Option<String>,
    /// `hired` or `removed`.
    pub status: String,
    #[serde(default)]
    pub created_at: String,
}

/// The hired professionals. `can_hire` says whether this device holds `APPROVE`,
/// so the team's buttons show only then. A host that does not send it counts as
/// no.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TeamHires {
    #[serde(default)]
    pub hires: Vec<Hire>,
    #[serde(default)]
    pub can_hire: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Onboarding {
    pub done: bool,
    #[serde(default)]
    pub areas: Vec<String>,
    #[serde(default)]
    pub can_hire: bool,
}

/// What the onboarding contracted. Nobody is hired after a skip.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OnboardingDone {
    pub done: bool,
    #[serde(default)]
    pub hires: Vec<Hire>,
}

/// One change to a hire. `Avatar(None)` removes the character: an absent
/// `avatar` means "no change" to the host, so the `null` has to be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HireChange {
    Name(String),
    Avatar(Option<String>),
}

#[derive(Debug, Deserialize)]
struct HireEnvelope {
    hire: Hire,
}

/// The route of one action on a hire. The id is checked first, so an id that is
/// not a hire id never leaves the device.
fn hire_route(hire_id: &str, action: &str) -> ChatResult<String> {
    validate_prefixed_id(hire_id, "hire_", "contratación")?;
    Ok(format!("/v1/hires/{hire_id}/{action}"))
}

impl HostClient {
    pub async fn team_roles(&self) -> ChatResult<RoleCatalog> {
        self.chat_get("/v1/roles", &[]).await
    }

    pub async fn team_hires(&self) -> ChatResult<TeamHires> {
        self.chat_get("/v1/hires", &[]).await
    }

    /// Contracts one role. A role already hired answers the hire it has, so a
    /// repeated call changes nothing.
    pub async fn team_hire(&self, role: &str) -> ChatResult<Hire> {
        let body = json!({ "role": role });
        let envelope: HireEnvelope = self
            .chat_send_json(Method::POST, "/v1/hires", &[], &body, true)
            .await?;
        Ok(envelope.hire)
    }

    pub async fn team_hire_change(&self, hire_id: &str, change: &HireChange) -> ChatResult<Hire> {
        let path = hire_route(hire_id, "update")?;
        let body = match change {
            HireChange::Name(name) => json!({ "name": name }),
            HireChange::Avatar(avatar) => json!({ "avatar": avatar }),
        };
        let envelope: HireEnvelope = self
            .chat_send_json(Method::POST, &path, &[], &body, true)
            .await?;
        Ok(envelope.hire)
    }

    pub async fn team_hire_dismiss(&self, hire_id: &str) -> ChatResult<Hire> {
        let path = hire_route(hire_id, "dismiss")?;
        let envelope: HireEnvelope = self
            .chat_send_json(
                Method::POST,
                &path,
                &[],
                &Value::Object(Default::default()),
                true,
            )
            .await?;
        Ok(envelope.hire)
    }

    pub async fn team_onboarding(&self) -> ChatResult<Onboarding> {
        self.chat_get("/v1/onboarding", &[]).await
    }

    /// Marks the onboarding done and contracts nobody.
    pub async fn team_onboarding_skip(&self) -> ChatResult<OnboardingDone> {
        self.chat_send_json(
            Method::POST,
            "/v1/onboarding",
            &[],
            &json!({ "skip": true }),
            true,
        )
        .await
    }

    /// Contracts exactly `roles`, each one inside `areas`, and marks the
    /// onboarding done.
    pub async fn team_onboarding_contract(
        &self,
        areas: &[String],
        roles: &[String],
    ) -> ChatResult<OnboardingDone> {
        let body = json!({ "areas": areas, "roles": roles });
        self.chat_send_json(Method::POST, "/v1/onboarding", &[], &body, true)
            .await
    }
}
