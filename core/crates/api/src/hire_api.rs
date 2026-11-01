//! The checks behind the hire routes (ONBOARD-T3). The catalogue decides what
//! may be hired and the store decides what may be kept, so nothing here writes.
//! The wire shape of a hire lives here too: every route answers with it.

use hn_core_chat::{Hire, NewHire};
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

#[cfg(test)]
mod tests {
    use super::*;

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
