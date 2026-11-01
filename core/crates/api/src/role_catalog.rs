//! The role catalogue behind `GET /v1/roles`: the professions a person can
//! hire, grouped by area, and the levels (ROLES §4, D3) that preselect them.
//!
//! The data is compiled in, so the host serves exactly the reviewed file. Its
//! source, `ROLES.md`, lives in another repository and is not read here: the
//! invariants in the tests are what the core relies on whatever that source says.

use serde::{Deserialize, Serialize};

const EMBEDDED: &str = include_str!("../data/roles.json");

/// The characters with artwork, without Alfred: he is fixed, never hired.
pub const CHARACTERS: [&str; 8] = [
    "bob", "conan", "dani", "ernesto", "hugo", "ines", "nicole", "sami",
];

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoleCatalog {
    pub version: u32,
    pub areas: Vec<Area>,
    pub roles: Vec<Role>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Area {
    pub id: String,
    pub label: String,
    /// Every role of the area, in catalogue order: the full "Contratar" list.
    pub roles: Vec<String>,
    pub levels: Levels,
}

/// Which roles each level preselects. Each list is inside the next one.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Levels {
    pub personal: Vec<String>,
    pub profesional: Vec<String>,
    pub equipo: Vec<String>,
}

/// The level a person picks for an area; each one preselects a list (D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Personal,
    Profesional,
    Equipo,
}

impl Levels {
    pub fn get(&self, level: Level) -> &[String] {
        match level {
            Level::Personal => &self.personal,
            Level::Profesional => &self.profesional,
            Level::Equipo => &self.equipo,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub title: String,
    pub area: String,
    /// The id of a character that has artwork, or `null`.
    pub avatar: Option<String>,
    pub apps: Vec<String>,
    pub skills: Skills,
}

/// Skills the role uses today, and those it still needs (ROLES §2).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Skills {
    pub ok: Vec<String>,
    pub pending: Vec<String>,
}

impl RoleCatalog {
    /// The catalogue compiled into this build.
    pub fn embedded() -> Result<Self, serde_json::Error> {
        serde_json::from_str(EMBEDDED)
    }

    pub fn role(&self, id: &str) -> Option<&Role> {
        self.roles.iter().find(|role| role.id == id)
    }

    pub fn area(&self, id: &str) -> Option<&Area> {
        self.areas.iter().find(|area| area.id == id)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const VERSION: u32 = 1;
    /// At most this many roles are visible in one area (D3).
    const MAX_VISIBLE: usize = 8;
    /// English words a Spanish title must not keep (D2). Terms that are in
    /// common use in Spanish (QA, SDET, VFX, SEO, CAD, DevOps, Android) are accepted.
    const ENGLISH_TERMS: &[&str] = &[
        "Assets",
        "Builder",
        "Copywriter",
        "Designer",
        "Drafter",
        "Engineer",
        "Analyst",
        "Lead",
        "Manager",
        "Motion",
        "Researcher",
        "Reviewer",
        "Sound",
        "Tester",
        "Video",
        "Writer",
    ];

    fn embedded() -> RoleCatalog {
        RoleCatalog::embedded().expect("the embedded catalogue parses")
    }

    fn is_snake_case(id: &str) -> bool {
        id.starts_with(|c: char| c.is_ascii_lowercase())
            && id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }

    fn subset(small: &[String], big: &[String]) -> bool {
        small.iter().all(|id| big.contains(id))
    }

    fn repeats<'a>(ids: impl Iterator<Item = &'a str>, kind: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        ids.filter(|id| !seen.insert(*id))
            .map(|id| format!("{kind} id {id} repeats"))
            .collect()
    }

    fn role_violations(catalog: &RoleCatalog, role: &Role) -> Vec<String> {
        let mut found = Vec::new();
        if !is_snake_case(&role.id) {
            found.push(format!("role id {} is not snake_case", role.id));
        }
        if role.name.trim().is_empty() || role.title.trim().is_empty() {
            found.push(format!("role {} has no name or no title", role.id));
        }
        let english = role
            .title
            .split(|c: char| !c.is_alphabetic())
            .find(|word| ENGLISH_TERMS.contains(word));
        if let Some(word) = english {
            found.push(format!(
                "role {} keeps the English title word {word}",
                role.id
            ));
        }
        if !catalog.areas.iter().any(|area| area.id == role.area) {
            found.push(format!("role {} is in unknown area {}", role.id, role.area));
        }
        if let Some(avatar) = role.avatar.as_deref().filter(|a| !CHARACTERS.contains(a)) {
            found.push(format!(
                "role {} has avatar {avatar}, not a character",
                role.id
            ));
        }
        found
    }

    fn area_violations(catalog: &RoleCatalog, area: &Area) -> Vec<String> {
        let mut found = Vec::new();
        let levels = &area.levels;
        if !is_snake_case(&area.id) {
            found.push(format!("area id {} is not snake_case", area.id));
        }
        if !subset(&levels.personal, &levels.profesional) {
            found.push(format!("{}: personal is not inside profesional", area.id));
        }
        if !subset(&levels.profesional, &levels.equipo) {
            found.push(format!("{}: profesional is not inside equipo", area.id));
        }
        if !subset(&levels.equipo, &area.roles) {
            found.push(format!(
                "{}: equipo is not inside the area's roles",
                area.id
            ));
        }
        if levels.equipo.len() > MAX_VISIBLE {
            found.push(format!(
                "{}: equipo has {} roles, at most {MAX_VISIBLE}",
                area.id,
                levels.equipo.len()
            ));
        }
        for id in &area.roles {
            match catalog.roles.iter().find(|role| &role.id == id) {
                None => found.push(format!("{}: role {id} does not exist", area.id)),
                Some(role) if role.area != area.id => {
                    found.push(format!("{}: role {id} belongs to {}", area.id, role.area));
                }
                Some(_) => {}
            }
        }
        for role in catalog.roles.iter().filter(|role| role.area == area.id) {
            if !area.roles.contains(&role.id) {
                found.push(format!(
                    "{}: role {} is missing from the area",
                    area.id, role.id
                ));
            }
        }
        found
    }

    /// Every broken invariant, as a sentence. Empty when the catalogue is sound.
    fn violations(catalog: &RoleCatalog) -> Vec<String> {
        let mut found = Vec::new();
        if catalog.version != VERSION {
            found.push(format!("version {} is not {VERSION}", catalog.version));
        }
        found.extend(repeats(catalog.areas.iter().map(|a| a.id.as_str()), "area"));
        found.extend(repeats(catalog.roles.iter().map(|r| r.id.as_str()), "role"));
        found.extend(repeats(
            catalog.roles.iter().filter_map(|r| r.avatar.as_deref()),
            "avatar",
        ));
        for role in &catalog.roles {
            found.extend(role_violations(catalog, role));
        }
        for area in &catalog.areas {
            found.extend(area_violations(catalog, area));
        }
        found
    }

    /// The embedded catalogue with one edit, to check the checker catches it.
    fn broken(edit: impl FnOnce(&mut RoleCatalog)) -> Vec<String> {
        let mut catalog = embedded();
        edit(&mut catalog);
        violations(&catalog)
    }

    #[test]
    fn the_embedded_catalogue_holds_every_invariant() {
        let found = violations(&embedded());
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn the_catalogue_has_the_reviewed_shape() {
        let catalog = embedded();
        assert_eq!(catalog.version, VERSION);
        assert_eq!(catalog.areas.len(), 9);
        assert_eq!(
            catalog.roles.len(),
            43,
            "44 roles of ROLES §2, minus Alfred"
        );
        assert!(catalog.roles.iter().all(|role| role.id != "alfred"));
        let avatars = catalog.roles.iter().filter(|r| r.avatar.is_some()).count();
        assert_eq!(avatars, CHARACTERS.len());
    }

    #[test]
    fn two_loads_serialise_identically() {
        let first = serde_json::to_string(&embedded()).expect("json");
        let second = serde_json::to_string(&embedded()).expect("json");
        assert_eq!(first, second);
    }

    #[test]
    fn the_checker_catches_a_repeated_role_id() {
        let found = broken(|catalog| {
            let copy = catalog.roles[0].clone();
            catalog.roles.push(copy);
        });
        assert!(found.iter().any(|v| v.contains("role id")), "{found:#?}");
    }

    #[test]
    fn the_checker_catches_more_than_eight_visible_roles() {
        let found = broken(|catalog| {
            let area = &mut catalog.areas[0];
            area.levels.equipo = area.roles.clone();
        });
        assert!(found.iter().any(|v| v.contains("at most 8")), "{found:#?}");
    }

    #[test]
    fn the_checker_catches_alfred_as_an_avatar() {
        let found = broken(|catalog| {
            let bob = catalog
                .roles
                .iter_mut()
                .find(|r| r.id == "bob")
                .expect("bob");
            bob.avatar = Some("alfred".into());
        });
        assert!(found.iter().any(|v| v.contains("alfred")), "{found:#?}");
    }

    #[test]
    fn the_checker_catches_an_english_title() {
        let found = broken(|catalog| {
            let bob = catalog
                .roles
                .iter_mut()
                .find(|r| r.id == "bob")
                .expect("bob");
            bob.title = "Builder".into();
        });
        assert!(found.iter().any(|v| v.contains("Builder")), "{found:#?}");
    }

    #[test]
    fn the_checker_catches_a_level_outside_its_area() {
        let found = broken(|catalog| {
            let video = catalog
                .areas
                .iter_mut()
                .find(|a| a.id == "video")
                .expect("video");
            video.levels.personal.push("bob".into());
        });
        assert!(
            found.iter().any(|v| v.starts_with("video: personal")),
            "{found:#?}"
        );
    }
}
