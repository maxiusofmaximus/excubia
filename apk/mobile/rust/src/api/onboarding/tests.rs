use serde_json::{json, Value};

use super::error::{explain_hire_error, explain_onboarding_error};
use super::model::{parse_hire, parse_hires, parse_onboarding_answer, parse_role_catalog};
use super::plan::{
    plan_hire, plan_hire_avatar, plan_hire_dismiss, plan_hire_rename, plan_hires,
    plan_onboarding_contract, plan_onboarding_read, plan_onboarding_skip, plan_roles,
};
use crate::api::chat::plan::ChatPlan;
use crate::api::error::FailureKind;

const CATALOG: &str = r#"{
  "ok": true, "version": 1, "extra": {"ignored": true},
  "areas": [
    { "id": "video", "label": "Vídeo y postproducción",
      "roles": ["vera", "jade"],
      "levels": { "personal": ["vera"], "profesional": ["vera", "jade"], "equipo": ["vera", "jade"] } },
    { "id": "otras", "label": "Otras", "roles": [],
      "levels": { "personal": [], "profesional": [], "equipo": [] } } ],
  "roles": [
    { "id": "vera", "name": "Vera", "title": "Editor de vídeo", "area": "video",
      "avatar": null, "apps": ["FilmCraft"], "skills": {"ok": [], "pending": []} },
    { "id": "bob", "name": "Bob", "title": "Desarrollador", "area": "video",
      "avatar": "bob", "novelty": 3 } ]
}"#;

fn body_of(plan: &ChatPlan) -> Value {
    let request = plan.request.as_ref().expect("the plan has a request");
    serde_json::from_slice(&request.body).expect("the body is JSON")
}

fn problem_of(plan: &ChatPlan) -> String {
    plan.problem
        .as_ref()
        .expect("the plan has a problem")
        .message
        .clone()
}

#[test]
fn catalogue_parses_areas_levels_and_roles() {
    let catalog = parse_role_catalog(CATALOG.to_string()).expect("a valid catalogue");
    assert_eq!(catalog.areas.len(), 2);
    let video = &catalog.areas[0];
    assert_eq!(video.label, "Vídeo y postproducción");
    assert_eq!(video.roles, ["vera", "jade"]);
    assert_eq!(video.personal, ["vera"]);
    assert_eq!(video.equipo, ["vera", "jade"]);
    assert_eq!(catalog.roles[0].avatar, None);
    assert_eq!(catalog.roles[1].avatar.as_deref(), Some("bob"));
}

#[test]
fn catalogue_refused_when_not_ok_or_missing_a_field() {
    let not_ok = CATALOG.replace("\"ok\": true", "\"ok\": false");
    assert_eq!(parse_role_catalog(not_ok), None);
    let no_levels = CATALOG.replace("\"levels\"", "\"lvls\"");
    assert_eq!(parse_role_catalog(no_levels), None);
    assert_eq!(parse_role_catalog("no es json".to_string()), None);
}

#[test]
fn onboarding_answer_reads_read_and_post_shapes() {
    let read = r#"{"ok": true, "done": true, "areas": ["video"]}"#;
    let answer = parse_onboarding_answer(read.to_string()).expect("a read");
    assert!(answer.done);
    assert_eq!(answer.areas, ["video"]);
    assert!(answer.hires.is_empty());

    let skipped = r#"{"ok": true, "done": true, "hires": []}"#;
    let answer = parse_onboarding_answer(skipped.to_string()).expect("a skip");
    assert!(answer.done);
    assert!(answer.areas.is_empty());
}

#[test]
fn hires_keep_removed_rows_and_ignore_unknown_fields() {
    let json = r#"{"ok": true, "hires": [
        {"id": "hire_1", "role": "bob", "area": "video", "name": "Roberto",
         "avatar": "ines", "status": "hired", "created_at": "2026-10-25T10:00:00.000Z", "x": 1},
        {"id": "hire_2", "role": "vera", "area": "video", "name": "Vera",
         "avatar": null, "status": "removed", "created_at": "2026-10-25T10:01:00.000Z"} ]}"#;
    let list = parse_hires(json.to_string()).expect("hires");
    assert_eq!(list.hires.len(), 2);
    assert_eq!(list.hires[0].avatar.as_deref(), Some("ines"));
    assert_eq!(list.hires[1].status, "removed");
    assert_eq!(list.hires[1].avatar, None);
}

#[test]
fn can_hire_is_read_and_missing_means_false() {
    let with = r#"{"ok": true, "hires": [], "can_hire": true}"#;
    assert!(parse_hires(with.to_string()).expect("hires").can_hire);
    let without = r#"{"ok": true, "hires": []}"#;
    assert!(!parse_hires(without.to_string()).expect("hires").can_hire);

    let read = r#"{"ok": true, "done": false, "areas": [], "can_hire": true}"#;
    assert!(
        parse_onboarding_answer(read.to_string())
            .expect("read")
            .can_hire
    );
    let old_host = r#"{"ok": true, "done": false, "areas": []}"#;
    assert!(
        !parse_onboarding_answer(old_host.to_string())
            .expect("read")
            .can_hire
    );
}

#[test]
fn one_hire_answer_is_read_from_the_hire_field() {
    let json = r#"{"ok": true, "hire": {"id": "hire_1", "role": "bob", "area": "video",
        "name": "Roberto", "avatar": "ines", "status": "hired",
        "created_at": "2026-10-25T10:00:00.000Z"}}"#;
    let hire = parse_hire(json.to_string()).expect("a hire");
    assert_eq!(hire.name, "Roberto");
    assert_eq!(hire.avatar.as_deref(), Some("ines"));
    assert!(parse_hire(r#"{"ok": false, "hire": {}}"#.to_string()).is_none());
}

#[test]
fn hire_plans_send_the_documented_bodies() {
    let hire = plan_hire("bob".into());
    assert_eq!(
        hire.request.as_ref().map(|r| r.target.as_str()),
        Some("/v1/hires")
    );
    assert_eq!(body_of(&hire), json!({"role": "bob"}));

    let rename = plan_hire_rename("hire_1".into(), "  Roberto  ".into());
    let request = rename.request.clone().expect("a request");
    assert_eq!(request.target, "/v1/hires/hire_1/update");
    assert_eq!(body_of(&rename), json!({"name": "Roberto"}));

    let character = plan_hire_avatar("hire_1".into(), Some("ines".into()));
    assert_eq!(body_of(&character), json!({"avatar": "ines"}));
    let initial = plan_hire_avatar("hire_1".into(), None);
    assert_eq!(body_of(&initial), json!({"avatar": null}));

    let dismiss = plan_hire_dismiss("hire_1".into());
    let request = dismiss.request.clone().expect("a request");
    assert_eq!(request.target, "/v1/hires/hire_1/dismiss");
    assert_eq!(body_of(&dismiss), json!({}));
}

#[test]
fn hire_plans_refuse_locally_what_the_host_would_refuse() {
    let bad_name = plan_hire_rename("hire_1".into(), "   ".into());
    assert!(bad_name.request.is_none());
    assert_eq!(
        problem_of(&bad_name),
        "El nombre debe tener entre 1 y 40 caracteres, sin caracteres de control."
    );

    let too_long = plan_hire_rename("hire_1".into(), "a".repeat(41));
    assert!(too_long.request.is_none());
    let forty = plan_hire_rename("hire_1".into(), "á".repeat(40));
    assert!(forty.request.is_some(), "40 characters count, not bytes");

    let control = plan_hire_rename("hire_1".into(), "Ana\u{7}".into());
    assert!(control.request.is_none());

    let alfred = plan_hire_avatar("hire_1".into(), Some("alfred".into()));
    assert!(alfred.request.is_none());
    assert_eq!(problem_of(&alfred), "Ese personaje no está disponible.");

    let bad_id = plan_hire_dismiss("hire/../x".into());
    assert!(bad_id.request.is_none());
    let bad_role = plan_hire("vi/deo".into());
    assert!(bad_role.request.is_none());
}

#[test]
fn hire_refusals_read_as_spanish_text() {
    let gone = explain_hire_error(
        404,
        r#"{"detail":{"code":"no_such_hire","message":"x"}}"#.into(),
    );
    assert_eq!(gone.message, "Ese agente ya no está en tu equipo.");
    let dismissed = explain_hire_error(
        409,
        r#"{"detail":{"code":"hire_not_active","message":"x"}}"#.into(),
    );
    assert_eq!(dismissed.message, "Ese agente ya fue despedido.");
    let avatar = explain_hire_error(
        422,
        r#"{"detail":{"code":"invalid_field","field":"avatar","message":"x"}}"#.into(),
    );
    assert_eq!(avatar.message, "Ese personaje no está disponible.");
    let name = explain_hire_error(
        422,
        r#"{"detail":{"code":"invalid_name","field":"name","message":"x"}}"#.into(),
    );
    assert!(name.message.contains("40 caracteres"));
    let full = explain_hire_error(
        409,
        r#"{"detail":{"code":"area_full","message":"x"}}"#.into(),
    );
    assert_eq!(full.message, "Esa área ya tiene 8 agentes.");
    let no_grant = explain_hire_error(403, String::new());
    assert_eq!(no_grant.kind, FailureKind::Permission);
    let revoked = explain_hire_error(401, String::new());
    assert_eq!(revoked.kind, FailureKind::Repair);
}

#[test]
fn reads_are_plain_gets_on_the_documented_routes() {
    for (plan, target) in [
        (plan_roles(), "/v1/roles"),
        (plan_onboarding_read(), "/v1/onboarding"),
        (plan_hires(false), "/v1/hires"),
        (plan_hires(true), "/v1/hires?include_removed=true"),
    ] {
        let request = plan.request.expect("a request");
        assert_eq!(request.method, "GET");
        assert_eq!(request.target, target);
        assert!(request.body.is_empty());
    }
}

#[test]
fn contract_sends_areas_level_and_the_exact_roles() {
    let plan =
        plan_onboarding_contract(vec!["video".into()], "personal".into(), vec!["vera".into()]);
    let request = plan.request.clone().expect("a request");
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/v1/onboarding");
    assert_eq!(
        body_of(&plan),
        json!({"areas": ["video"], "level": "personal", "roles": ["vera"]})
    );
}

#[test]
fn contract_refuses_locally_what_the_host_would_refuse() {
    let no_area = plan_onboarding_contract(vec![], "personal".into(), vec!["vera".into()]);
    assert!(no_area.request.is_none());
    assert_eq!(problem_of(&no_area), "Elige al menos un área.");

    let no_role = plan_onboarding_contract(vec!["video".into()], "personal".into(), vec![]);
    assert!(no_role.request.is_none());
    assert_eq!(
        problem_of(&no_role),
        "Elige al menos una persona para contratar."
    );

    let bad_level =
        plan_onboarding_contract(vec!["video".into()], "todo".into(), vec!["vera".into()]);
    assert!(bad_level.request.is_none());

    let bad_id = plan_onboarding_contract(
        vec!["vi/deo".into()],
        "personal".into(),
        vec!["vera".into()],
    );
    assert!(bad_id.request.is_none());
}

#[test]
fn skip_body_is_fixed() {
    let skip = plan_onboarding_skip();
    assert_eq!(
        skip.request.as_ref().map(|r| r.method.as_str()),
        Some("POST")
    );
    assert_eq!(body_of(&skip), json!({"skip": true}));
}

#[test]
fn a_missing_grant_asks_for_approve_and_needs_permission() {
    let failure = explain_onboarding_error(403, String::new());
    assert_eq!(failure.kind, FailureKind::Permission);
    assert!(failure.message.contains("permiso"));
    assert!(failure
        .action
        .as_deref()
        .unwrap_or_default()
        .contains("APPROVE"));
}

#[test]
fn hire_and_onboarding_codes_read_as_spanish_text() {
    let full = explain_onboarding_error(
        409,
        r#"{"detail":{"code":"area_full","message":"x"}}"#.into(),
    );
    assert_eq!(full.message, "Esa área ya tiene 8 agentes.");
    assert_eq!(full.kind, FailureKind::Retry);

    let unknown = explain_onboarding_error(
        422,
        r#"{"detail":{"code":"unknown_role","field":"role","message":"x"}}"#.into(),
    );
    assert_eq!(unknown.message, "El catálogo de roles ha cambiado.");

    let outside = explain_onboarding_error(
        422,
        r#"{"detail":{"code":"invalid_field","field":"roles","message":"x"}}"#.into(),
    );
    assert_eq!(
        outside.message,
        "Uno de los roles no pertenece a las áreas elegidas."
    );

    let no_area = explain_onboarding_error(
        422,
        r#"{"detail":{"code":"invalid_field","field":"areas","message":"x"}}"#.into(),
    );
    assert_eq!(no_area.message, "Elige al menos un área.");
}

#[test]
fn unknown_refusals_fall_back_to_the_generic_text() {
    let revoked = explain_onboarding_error(401, String::new());
    assert_eq!(revoked.kind, FailureKind::Repair);

    let odd = explain_onboarding_error(
        422,
        r#"{"detail":{"code":"invalid_name","field":"name","message":"x"}}"#.into(),
    );
    assert_eq!(odd.message, "La petición no es válida.");
}
