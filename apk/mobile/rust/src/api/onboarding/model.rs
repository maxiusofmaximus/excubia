//! The role catalogue, the onboarding answer and the hires as the host sends
//! them. Unknown fields are ignored, so a newer host can add to them without
//! breaking the app; a missing required field makes the whole answer `None`.

use serde::Deserialize;

/// The professions a person can hire, grouped by area (`GET /v1/roles`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleCatalog {
    pub areas: Vec<RoleArea>,
    pub roles: Vec<CatalogRole>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleArea {
    pub id: String,
    pub label: String,
    /// Every role of the area, in the host's order.
    pub roles: Vec<String>,
    /// The roles each onboarding level preselects. `equipo` never exceeds 8.
    pub personal: Vec<String>,
    pub profesional: Vec<String>,
    pub equipo: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRole {
    pub id: String,
    pub name: String,
    pub title: String,
    pub area: String,
    /// The id of a character with artwork; `None` means the initial instead.
    pub avatar: Option<String>,
}

/// `GET /v1/onboarding` leaves `hires` empty; the `POST` lists the hires it made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnboardingAnswer {
    pub done: bool,
    pub areas: Vec<String>,
    pub hires: Vec<Hire>,
    /// The device holds `APPROVE`. A missing field reads as `false`.
    pub can_hire: bool,
}

/// `GET /v1/hires`: the hires, and whether this device may change them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HireList {
    pub hires: Vec<Hire>,
    pub can_hire: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hire {
    pub id: String,
    pub role: String,
    pub area: String,
    pub name: String,
    pub avatar: Option<String>,
    /// `hired`, or `removed` once dismissed.
    pub status: String,
    pub created_at: String,
}

#[derive(Deserialize)]
struct WireCatalog {
    ok: bool,
    areas: Vec<WireArea>,
    roles: Vec<WireRole>,
}

#[derive(Deserialize)]
struct WireArea {
    id: String,
    label: String,
    roles: Vec<String>,
    levels: WireLevels,
}

#[derive(Deserialize)]
struct WireLevels {
    personal: Vec<String>,
    profesional: Vec<String>,
    equipo: Vec<String>,
}

#[derive(Deserialize)]
struct WireRole {
    id: String,
    name: String,
    title: String,
    area: String,
    avatar: Option<String>,
}

#[derive(Deserialize)]
struct WireAnswer {
    ok: bool,
    done: bool,
    #[serde(default)]
    areas: Vec<String>,
    #[serde(default)]
    hires: Vec<WireHire>,
    #[serde(default)]
    can_hire: bool,
}

#[derive(Deserialize)]
struct WireHires {
    ok: bool,
    hires: Vec<WireHire>,
    #[serde(default)]
    can_hire: bool,
}

#[derive(Deserialize)]
struct WireOneHire {
    ok: bool,
    hire: WireHire,
}

#[derive(Deserialize)]
struct WireHire {
    id: String,
    role: String,
    area: String,
    name: String,
    avatar: Option<String>,
    status: String,
    created_at: String,
}

impl From<WireArea> for RoleArea {
    fn from(area: WireArea) -> Self {
        Self {
            id: area.id,
            label: area.label,
            roles: area.roles,
            personal: area.levels.personal,
            profesional: area.levels.profesional,
            equipo: area.levels.equipo,
        }
    }
}

impl From<WireRole> for CatalogRole {
    fn from(role: WireRole) -> Self {
        Self {
            id: role.id,
            name: role.name,
            title: role.title,
            area: role.area,
            avatar: role.avatar,
        }
    }
}

impl From<WireHire> for Hire {
    fn from(hire: WireHire) -> Self {
        Self {
            id: hire.id,
            role: hire.role,
            area: hire.area,
            name: hire.name,
            avatar: hire.avatar,
            status: hire.status,
            created_at: hire.created_at,
        }
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_role_catalog(json: String) -> Option<RoleCatalog> {
    let wire: WireCatalog = serde_json::from_str(&json).ok()?;
    if !wire.ok {
        return None;
    }
    Some(RoleCatalog {
        areas: wire.areas.into_iter().map(RoleArea::from).collect(),
        roles: wire.roles.into_iter().map(CatalogRole::from).collect(),
    })
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_onboarding_answer(json: String) -> Option<OnboardingAnswer> {
    let wire: WireAnswer = serde_json::from_str(&json).ok()?;
    if !wire.ok {
        return None;
    }
    Some(OnboardingAnswer {
        done: wire.done,
        areas: wire.areas,
        hires: wire.hires.into_iter().map(Hire::from).collect(),
        can_hire: wire.can_hire,
    })
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_hires(json: String) -> Option<HireList> {
    let wire: WireHires = serde_json::from_str(&json).ok()?;
    if !wire.ok {
        return None;
    }
    Some(HireList {
        hires: wire.hires.into_iter().map(Hire::from).collect(),
        can_hire: wire.can_hire,
    })
}

/// The one hire that `create`, `rename`, `avatar` and `dismiss` answer with.
#[flutter_rust_bridge::frb(sync)]
pub fn parse_hire(json: String) -> Option<Hire> {
    let wire: WireOneHire = serde_json::from_str(&json).ok()?;
    if !wire.ok {
        return None;
    }
    Some(Hire::from(wire.hire))
}
