//! The checks behind the hire routes (ONBOARD-T3). The catalogue decides what
//! may be hired and the store decides what may be kept, so nothing here writes.
//! The wire shape of a hire lives here too: every route answers with it.

use hn_core_chat::{activity_of, Hire, HireLink, MemberInput, NewHire};
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::role_catalog::{Area, Role, RoleCatalog, CHARACTERS};

/// The catalogue, parsed for this request. It is about 140 lines, so parsing
/// it per request costs less than keeping it behind a lock.
pub fn catalog() -> Result<RoleCatalog, ApiError> {
    RoleCatalog::embedded().map_err(ApiError::internal)
}

pub fn role<'a>(catalog: &'a RoleCatalog, id: &str) -> Result<&'a Role, ApiError> {
    catalog.role(id).ok_or_else(|| {
        ApiError::invalid_field("unknown_role", "role", format!("unknown role {id:?}"))
    })
}

pub fn area<'a>(catalog: &'a RoleCatalog, id: &str) -> Result<&'a Area, ApiError> {
    catalog.area(id).ok_or_else(|| {
        ApiError::invalid_field("unknown_area", "areas", format!("unknown area {id:?}"))
    })
}

/// A character id with artwork. Alfred is not one: he is fixed, never hired.
pub fn character(id: &str) -> Result<String, ApiError> {
    if CHARACTERS.contains(&id) {
        Ok(id.to_string())
    } else {
        Err(ApiError::invalid_field(
            "invalid_field",
            "avatar",
            format!("{id:?} is not a character"),
        ))
    }
}

/// The avatar a new hire gets: the one asked for, or else the role's own.
pub fn avatar_for(role: &Role, requested: Option<&str>) -> Result<Option<String>, ApiError> {
    match requested {
        Some(id) => character(id).map(Some),
        None => Ok(role.avatar.clone()),
    }
}

/// A hire to contract for a catalogue role. The name defaults to the role's.
pub fn new_hire(role: &Role, name: Option<String>, avatar: Option<String>) -> NewHire {
    NewHire {
        catalog_role: role.id.clone(),
        area: role.area.clone(),
        name: name.unwrap_or_else(|| role.name.clone()),
        avatar,
        engine: None,
        profile: None,
        skills_allow: Vec::new(),
    }
}

pub fn hire_json(hire: &Hire) -> Value {
    json!({
        "id": hire.id,
        "role": hire.catalog_role,
        "area": hire.area,
        "name": hire.name,
        "avatar": hire.avatar,
        "status": hire.status.as_str(),
        "created_at": hire.created_at,
    })
}

/// One member of the team as the summary reads it: the hire's role title, and
/// what its seat is doing (`blocked` comes from the pane, the rest from the chat).
pub fn member_input(
    link: &HireLink,
    blocked: bool,
    catalog: &RoleCatalog,
    now_ms: i64,
) -> Result<MemberInput, ApiError> {
    let role = catalog.role(&link.catalog_role).ok_or_else(|| {
        ApiError::internal(format!("hire {} has no catalogue role", link.hire_id))
    })?;
    let (activity, since) = activity_of(link, blocked, now_ms);
    Ok(MemberInput {
        agent_id: link.hire_id.clone(),
        name: link.name.clone(),
        role_title: role.title.clone(),
        since,
        activity,
    })
}

#[cfg(test)]
mod tests {
    use hn_core_chat::{summarize, TeamStatus};

    use super::*;

    const NOW_MS: i64 = 1_793_880_000_000; // 2026-11-05T12:00:00Z

    fn hired(id: &str, role: &str) -> HireLink {
        HireLink {
            hire_id: id.into(),
            name: "Bob".into(),
            catalog_role: role.into(),
            hired_at: "2026-11-05T10:00:00.000Z".into(),
            seat_pane: None,
            approval: None,
            working_since: None,
            finished_at: None,
            plan: None,
        }
    }

    #[test]
    fn a_hire_without_a_seat_is_idle_since_it_was_hired() {
        let catalog = catalog().expect("catalogue");
        let input = member_input(&hired("hire_1", "bob"), false, &catalog, NOW_MS).expect("input");
        let members = summarize(&[input]);
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].agent_id, "hire_1");
        assert_eq!(
            members[0].role_title,
            role(&catalog, "bob").expect("bob").title
        );
        assert_eq!(members[0].status, TeamStatus::Idle);
        assert_eq!(members[0].headline, "Sin actividad");
        assert_eq!(members[0].since, "2026-11-05T10:00:00.000Z");
    }

    #[test]
    fn a_hire_of_a_role_that_is_gone_is_an_internal_error() {
        let catalog = catalog().expect("catalogue");
        let error =
            member_input(&hired("hire_1", "nope"), false, &catalog, NOW_MS).expect_err("gone");
        assert_eq!(error.status, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    fn code(error: &ApiError) -> &str {
        error.body["detail"]["code"].as_str().expect("code")
    }

    #[test]
    fn only_the_eight_characters_are_avatars() {
        assert_eq!(character("bob").expect("bob"), "bob");
        assert_eq!(
            code(&character("alfred").expect_err("alfred")),
            "invalid_field"
        );
        assert_eq!(code(&character("Bob").expect_err("case")), "invalid_field");
    }

    #[test]
    fn a_role_without_a_character_gets_no_avatar() {
        let catalog = catalog().expect("catalogue");
        let vera = role(&catalog, "vera").expect("vera");
        assert_eq!(avatar_for(vera, None).expect("default"), None);
        let bob = role(&catalog, "bob").expect("bob");
        assert_eq!(avatar_for(bob, None).expect("default"), Some("bob".into()));
    }

    #[test]
    fn unknown_roles_and_areas_are_named_by_their_codes() {
        let catalog = catalog().expect("catalogue");
        assert_eq!(
            code(&role(&catalog, "alfred").expect_err("role")),
            "unknown_role"
        );
        assert_eq!(
            code(&area(&catalog, "nope").expect_err("area")),
            "unknown_area"
        );
    }
}
