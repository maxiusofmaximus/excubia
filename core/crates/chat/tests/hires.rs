//! ONBOARD-T1 store rules: the hires, their names and avatars, the area limit,
//! the team that is contracted whole, and the onboarding state. The seats of a
//! project keep their own limit: a hire never takes one.

mod common;

use common::team;
use excubia_core_chat::{
    ChatError, ChatStore, HireError, HireFilter, HireStatus, NameError, NewHire, NewSeat,
    OnboardingState, MAX_HIRES_PER_AREA,
};

fn new_hire(role: &str, area: &str, name: &str) -> NewHire {
    NewHire {
        catalog_role: role.to_string(),
        area: area.to_string(),
        name: name.to_string(),
        avatar: None,
        engine: None,
        profile: None,
        skills_allow: Vec::new(),
    }
}

fn fresh_store() -> ChatStore {
    ChatStore::in_memory().expect("store")
}

#[test]
fn hires_are_listed_in_creation_order_and_dismissed_ones_are_hidden_by_default() {
    let store = fresh_store();
    let ines = store
        .hire(new_hire("audio-editor", "audio", "Ines"))
        .expect("hire")
        .into_hire();
    let bruno = store
        .hire(new_hire("video-editor", "video", "Bruno"))
        .expect("hire")
        .into_hire();
    store.dismiss_hire(&ines.id).expect("dismiss");

    let active = store.list_hires(HireFilter::Active).expect("active");
    let active_ids: Vec<_> = active.iter().map(|hire| hire.id.as_str()).collect();
    assert_eq!(active_ids, vec![bruno.id.as_str()]);

    let all = store.list_hires(HireFilter::All).expect("all");
    let all_ids: Vec<_> = all.iter().map(|hire| hire.id.as_str()).collect();
    assert_eq!(all_ids, vec![ines.id.as_str(), bruno.id.as_str()]);
    assert_eq!(all[0].status, HireStatus::Removed);
}

#[test]
fn names_are_trimmed_and_invalid_ones_change_nothing() {
    let store = fresh_store();
    let hire = store
        .hire(new_hire("audio-editor", "audio", "  Ines  "))
        .expect("hire")
        .into_hire();
    assert_eq!(hire.name, "Ines");
    let renamed = store.rename_hire(&hire.id, "Inés Ríos").expect("rename");
    assert_eq!(renamed.name, "Inés Ríos");

    let at_limit = "ñ".repeat(40);
    assert_eq!(
        store
            .rename_hire(&hire.id, &at_limit)
            .expect("40 characters")
            .name,
        at_limit
    );
    assert!(matches!(
        store.rename_hire(&hire.id, "   "),
        Err(HireError::InvalidName(NameError::Empty))
    ));
    assert!(matches!(
        store.rename_hire(&hire.id, &"a".repeat(41)),
        Err(HireError::InvalidName(NameError::TooLong))
    ));
    assert!(matches!(
        store.rename_hire(&hire.id, "Ines\u{7}"),
        Err(HireError::InvalidName(NameError::ControlCharacters))
    ));
    assert!(matches!(
        store.hire(new_hire("video-editor", "video", "\n")),
        Err(HireError::InvalidName(NameError::Empty))
    ));
    assert_eq!(store.get_hire(&hire.id).expect("get").name, at_limit);
    assert_eq!(store.list_hires(HireFilter::All).expect("all").len(), 1);
}

#[test]
fn the_avatar_is_a_character_id_independent_of_the_name() {
    let store = fresh_store();
    let mut new = new_hire("audio-editor", "audio", "Ines");
    new.avatar = Some("ines".to_string());
    let hire = store.hire(new).expect("hire").into_hire();

    let renamed = store.rename_hire(&hire.id, "Inés").expect("rename");
    assert_eq!(renamed.avatar.as_deref(), Some("ines"));

    let plain = store.set_hire_avatar(&hire.id, None).expect("avatar");
    assert_eq!(plain.avatar, None);
    assert_eq!(plain.name, "Inés");
}

#[test]
fn dismissing_is_idempotent_and_a_dismissed_hire_is_read_only() {
    let store = fresh_store();
    let hire = store
        .hire(new_hire("audio-editor", "audio", "Ines"))
        .expect("hire")
        .into_hire();
    let dismissed = store.dismiss_hire(&hire.id).expect("dismiss");
    assert_eq!(dismissed.status, HireStatus::Removed);
    assert!(dismissed.removed_at.is_some());

    let again = store.dismiss_hire(&hire.id).expect("dismiss twice");
    assert_eq!(again, dismissed, "the second dismissal changes nothing");

    assert!(matches!(
        store.rename_hire(&hire.id, "Otra"),
        Err(HireError::HireNotActive(_))
    ));
    assert!(matches!(
        store.set_hire_avatar(&hire.id, Some("ines")),
        Err(HireError::HireNotActive(_))
    ));
    assert!(matches!(
        store.dismiss_hire("hire_missing"),
        Err(HireError::NoSuchHire(_))
    ));
}

#[test]
fn an_area_holds_eight_active_hires_and_a_dismissal_frees_a_place() {
    let store = fresh_store();
    let first = store
        .hire(new_hire("audio-0", "audio", "Persona 0"))
        .expect("within the limit")
        .into_hire();
    for index in 1..MAX_HIRES_PER_AREA {
        store
            .hire(new_hire(
                &format!("audio-{index}"),
                "audio",
                &format!("Persona {index}"),
            ))
            .expect("within the limit")
            .into_hire();
    }
    let error = store
        .hire(new_hire("audio-extra", "audio", "Demasiada"))
        .expect_err("the area is full");
    assert!(
        matches!(error, HireError::AreaFull(ref area) if area == "audio"),
        "{error}"
    );

    store
        .hire(new_hire("video-0", "video", "Otra área"))
        .expect("another area is not affected");

    store.dismiss_hire(&first.id).expect("dismiss");
    store
        .hire(new_hire("audio-extra", "audio", "Ahora sí"))
        .expect("the dismissed place is free");
}

#[test]
fn a_team_is_contracted_whole_or_not_at_all() {
    let store = fresh_store();
    for index in 0..(MAX_HIRES_PER_AREA - 1) {
        store
            .hire(new_hire(
                &format!("audio-{index}"),
                "audio",
                &format!("Persona {index}"),
            ))
            .expect("hire")
            .into_hire();
    }
    let team = [
        new_hire("audio-new-1", "audio", "Uno"),
        new_hire("audio-new-2", "audio", "Dos"),
    ];
    let error = store
        .hire_team(&team)
        .expect_err("the second one does not fit");
    assert!(matches!(error, HireError::AreaFull(_)), "{error}");
    let active = store.list_hires(HireFilter::Active).expect("list");
    assert_eq!(
        active.len(),
        MAX_HIRES_PER_AREA - 1,
        "none of the team was kept"
    );

    let fresh = fresh_store();
    let broken = [
        new_hire("audio-editor", "audio", "Ines"),
        new_hire("video-editor", "video", "\u{7}"),
        new_hire("datos-analyst", "datos", "Carla"),
    ];
    assert!(matches!(
        fresh.hire_team(&broken),
        Err(HireError::InvalidName(_))
    ));
    assert!(fresh.list_hires(HireFilter::All).expect("list").is_empty());
}

#[test]
fn a_team_never_hires_the_same_role_twice() {
    let store = fresh_store();
    let existing = store
        .hire(new_hire("audio-editor", "audio", "Ines"))
        .expect("hire")
        .into_hire();
    let team = store
        .hire_team(&[
            new_hire("audio-editor", "audio", "Otra"),
            new_hire("video-editor", "video", "Bruno"),
            new_hire("video-editor", "video", "Bruno bis"),
        ])
        .expect("team");

    assert_eq!(team.len(), 3);
    assert_eq!(team[0], existing, "the active hire comes back unchanged");
    assert_eq!(team[1].id, team[2].id);
    assert_eq!(store.list_hires(HireFilter::Active).expect("list").len(), 2);
}

#[test]
fn skills_engine_and_profile_are_kept() {
    let store = fresh_store();
    let hire = store
        .hire(new_hire("datos-analyst", "datos", "Carla"))
        .expect("hire")
        .into_hire();
    assert!(hire.skills_allow.is_empty());
    assert_eq!(hire.engine, None);

    let mut new = new_hire("video-editor", "video", "Bruno");
    new.engine = Some("claude".to_string());
    new.profile = Some("video".to_string());
    new.skills_allow = vec!["ffmpeg".to_string(), "subtitulos".to_string()];
    let stored = store.hire(new).expect("hire").into_hire();
    let read = store.get_hire(&stored.id).expect("get");
    assert_eq!(read.engine.as_deref(), Some("claude"));
    assert_eq!(read.profile.as_deref(), Some("video"));
    assert_eq!(read.skills_allow, vec!["ffmpeg", "subtitulos"]);
}

#[test]
fn the_onboarding_is_recorded_once() {
    let store = fresh_store();
    let initial = store.onboarding_state().expect("state");
    assert_eq!(
        initial,
        OnboardingState {
            done: false,
            areas: Vec::new(),
        }
    );

    let areas = vec!["audio".to_string(), "video".to_string()];
    let done = store.complete_onboarding(&areas).expect("complete");
    assert!(done.done);
    assert_eq!(done.areas, areas);

    let second = store
        .complete_onboarding(&["datos".to_string()])
        .expect("complete again");
    assert_eq!(second, done, "asking again changes nothing");
    assert_eq!(store.onboarding_state().expect("state"), done);
}

#[test]
fn hires_do_not_take_the_seats_of_a_project() {
    let t = team("hires-seats");
    for index in 0..MAX_HIRES_PER_AREA {
        t.store
            .hire(new_hire(
                &format!("audio-{index}"),
                "audio",
                &format!("Persona {index}"),
            ))
            .expect("hire")
            .into_hire();
    }

    // The lead and Ana already count: six more seats fill the project.
    for index in 0..6 {
        t.store
            .create_seat(NewSeat {
                project_id: t.project.clone(),
                name: format!("Extra {index}"),
                role_id: "specialist".to_string(),
            })
            .expect("seat");
    }
    let error = t
        .store
        .create_seat(NewSeat {
            project_id: t.project.clone(),
            name: "Demasiado".to_string(),
            role_id: "specialist".to_string(),
        })
        .expect_err("seat limit");
    assert!(matches!(error, ChatError::SeatLimit), "{error}");
}
