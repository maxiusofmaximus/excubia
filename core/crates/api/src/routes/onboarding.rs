//! `/v1/onboarding` (ONBOARD-T3): the first-run question «¿Para qué lo vas a
//! usar?». Finishing it contracts the team of the chosen areas, and it is the
//! user's `APPROVE`. Asking again contracts nothing new. Skipping it marks it
//! done and contracts nobody.

use axum::body::Bytes;
use axum::extract::{Extension, State};
use axum::Json;
use hn_core_auth::{Permission, VerifiedDevice};
use hn_core_chat::NewHire;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::hire_api::{self, hire_json};
use crate::role_catalog::{Area, Level, Role, RoleCatalog};
use crate::routes::parse_body;
use crate::ApiContext;

#[derive(Deserialize)]
struct OnboardingBody {
    /// Not needed when skipping.
    #[serde(default)]
    areas: Vec<String>,
    #[serde(default)]
    level: Option<Level>,
    /// The user's final selection. When present, the level is not used.
    #[serde(default)]
    roles: Option<Vec<String>>,
    /// Marks the onboarding done without contracting anyone. Refused together
    /// with a selection.
    #[serde(default)]
    skip: bool,
}

/// `can_hire` tells the app whether this device may contract: it reports the
/// grant the device already has and changes no authorization.
pub async fn state(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
) -> Result<Json<Value>, ApiError> {
    let onboarding = ctx.chat.onboarding_state()?;
    Ok(Json(json!({
        "ok": true,
        "done": onboarding.done,
        "areas": onboarding.areas,
        "can_hire": verified.has(Permission::Approve),
    })))
}

/// The team is contracted in one transaction, so a refused limit leaves no
/// hire behind. The state is marked after it: a retry after a failure in
/// between contracts nothing new and only marks it.
pub async fn complete(State(ctx): State<ApiContext>, body: Bytes) -> Result<Json<Value>, ApiError> {
    let payload: OnboardingBody = parse_body(&body)?;
    let catalog = hire_api::catalog()?;
    let team = team_for(&catalog, &payload)?;
    let hires = ctx.chat.hire_team(&team)?;
    let onboarding = ctx.chat.complete_onboarding(&payload.areas)?;
    Ok(Json(json!({
        "ok": true,
        "done": onboarding.done,
        "hires": hires.iter().map(hire_json).collect::<Vec<_>>(),
    })))
}

/// The team an onboarding contracts: nobody when the user skips it, the user's
/// own roles when they sent them, otherwise each area's list for the level (D3,
/// `profesional` by default).
fn team_for(catalog: &RoleCatalog, payload: &OnboardingBody) -> Result<Vec<NewHire>, ApiError> {
    let picked_roles = payload.roles.as_deref().is_some_and(|ids| !ids.is_empty());
    if payload.skip {
        if !payload.areas.is_empty() || picked_roles {
            return Err(ApiError::invalid_field(
                "invalid_field",
                "skip",
                "skip cannot be sent with areas or roles",
            ));
        }
        return Ok(Vec::new());
    }
    if payload.areas.is_empty() {
        return Err(ApiError::invalid_field(
            "invalid_field",
            "areas",
            "choose at least one area",
        ));
    }
    let areas = payload
        .areas
        .iter()
        .map(|id| hire_api::area(catalog, id))
        .collect::<Result<Vec<&Area>, ApiError>>()?;
    let roles = match &payload.roles {
        Some(ids) => ids
            .iter()
            .map(|id| selected_role(catalog, &areas, id))
            .collect::<Result<Vec<&Role>, ApiError>>()?,
        None => {
            let level = payload.level.unwrap_or(Level::Profesional);
            areas
                .iter()
                .flat_map(|area| area.levels.get(level).iter())
                .map(|id| hire_api::role(catalog, id))
                .collect::<Result<Vec<&Role>, ApiError>>()?
        }
    };
    Ok(roles
        .into_iter()
        .map(|role| hire_api::new_hire(role, None, role.avatar.clone()))
        .collect())
}

/// A role the user picked must exist and belong to one of the chosen areas.
fn selected_role<'a>(
    catalog: &'a RoleCatalog,
    areas: &[&Area],
    id: &str,
) -> Result<&'a Role, ApiError> {
    let role = hire_api::role(catalog, id)?;
    if areas.iter().any(|area| area.id == role.area) {
        Ok(role)
    } else {
        Err(ApiError::invalid_field(
            "invalid_field",
            "roles",
            format!("role {id:?} is not in the chosen areas"),
        ))
    }
}
