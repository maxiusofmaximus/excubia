//! `/v1/roles`, `/v1/hires` and `/v1/onboarding` request plans. Route, query and
//! body are built and checked here; Dart only signs and sends the bytes.

use serde_json::{json, Value};

use crate::api::chat::plan::{get, post, ChatPlan};
use crate::api::error::{info, FailureKind};
use crate::api::validate;

const ONBOARDING: &str = "/v1/onboarding";
const HIRES: &str = "/v1/hires";
/// The levels of `POST /v1/onboarding`, from the smallest team to the largest.
const LEVELS: [&str; 3] = ["personal", "profesional", "equipo"];
/// The characters with artwork (`CHARACTERS` in the host's `role_catalog.rs`).
/// Alfred is fixed and never hired, so he is not one of them.
const CHARACTERS: [&str; 8] = [
    "bob", "conan", "dani", "ernesto", "hugo", "ines", "nicole", "sami",
];
/// `errors.md`: a name is 1 to 40 characters with no control characters.
const NAME_MAX_CHARS: usize = 40;
const BAD_HIRE: &str = "Ese agente no es válido.";

#[flutter_rust_bridge::frb(sync)]
pub fn plan_roles() -> ChatPlan {
    get("/v1/roles".to_string())
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_hires(include_removed: bool) -> ChatPlan {
    let target = if include_removed {
        "/v1/hires?include_removed=true"
    } else {
        "/v1/hires"
    };
    get(target.to_string())
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_onboarding_read() -> ChatPlan {
    get(ONBOARDING.to_string())
}

/// Finishes the onboarding with the exact team. `roles` must not be empty: when
/// it is left out, the host contracts each area's list for `level` instead.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_onboarding_contract(areas: Vec<String>, level: String, roles: Vec<String>) -> ChatPlan {
    if areas.is_empty() {
        return problem("Elige al menos un área.");
    }
    if roles.is_empty() {
        return problem("Elige al menos una persona para contratar.");
    }
    if !LEVELS.contains(&level.as_str()) || !all_ids(&areas) || !all_ids(&roles) {
        return problem("La petición no es válida.");
    }
    json_post(
        ONBOARDING,
        json!({ "areas": areas, "level": level, "roles": roles }),
    )
}

/// Marks the onboarding done without contracting anyone (the «Saltar» of the
/// first screen). A second skip changes nothing.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_onboarding_skip() -> ChatPlan {
    json_post(ONBOARDING, json!({ "skip": true }))
}

/// Contracts one role of the catalogue with its default name and avatar. The
/// host answers `200` when the role is already hired.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_hire(role: String) -> ChatPlan {
    if !validate::identifier(&role) {
        return problem("Ese profesional no está en el catálogo.");
    }
    json_post(HIRES, json!({ "role": role }))
}

/// Renames a hire. The name is trimmed and checked here first; the host has the
/// final word.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_hire_rename(hire_id: String, name: String) -> ChatPlan {
    if !validate::identifier(&hire_id) {
        return problem(BAD_HIRE);
    }
    match clean_name(&name) {
        Some(name) => json_post(&hire_path(&hire_id, "update"), json!({ "name": name })),
        None => problem("El nombre debe tener entre 1 y 40 caracteres, sin caracteres de control."),
    }
}

/// Changes a hire's character. `None` asks for no character (the initial).
#[flutter_rust_bridge::frb(sync)]
pub fn plan_hire_avatar(hire_id: String, avatar: Option<String>) -> ChatPlan {
    if !validate::identifier(&hire_id) {
        return problem(BAD_HIRE);
    }
    if avatar
        .as_deref()
        .is_some_and(|id| !CHARACTERS.contains(&id))
    {
        return problem("Ese personaje no está disponible.");
    }
    json_post(&hire_path(&hire_id, "update"), json!({ "avatar": avatar }))
}

/// Dismisses a hire. The host answers the same hire when it is dismissed twice.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_hire_dismiss(hire_id: String) -> ChatPlan {
    if !validate::identifier(&hire_id) {
        return problem(BAD_HIRE);
    }
    json_post(&hire_path(&hire_id, "dismiss"), json!({}))
}

fn json_post(target: &str, body: Value) -> ChatPlan {
    post(
        target.to_string(),
        body.as_object().cloned().unwrap_or_default(),
    )
}

fn hire_path(hire_id: &str, action: &str) -> String {
    format!("{HIRES}/{hire_id}/{action}")
}

/// The trimmed name when the host would accept it.
fn clean_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    let length = name.chars().count();
    let fits = (1..=NAME_MAX_CHARS).contains(&length);
    (fits && !name.chars().any(char::is_control)).then(|| name.to_string())
}

fn all_ids(ids: &[String]) -> bool {
    ids.iter().all(|id| validate::identifier(id))
}

fn problem(message: &str) -> ChatPlan {
    ChatPlan {
        request: None,
        problem: Some(info(message, None, FailureKind::Retry)),
    }
}
