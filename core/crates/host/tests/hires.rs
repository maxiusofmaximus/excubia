//! The team through the real host (ONBOARD-T3): contracting registers
//! professionals, the store's rules and the catalogue hold over HTTP, and only
//! the user's `APPROVE` writes the team.

mod harness;

use harness::{raw_exchange, TestHost};
use hn_core_auth::Permission;
use hn_core_test_support::SignedClient;
use serde_json::{json, Value};

/// One signed request, answered as the status and the parsed body.
async fn call(
    client: &SignedClient,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let bytes = body.map(|value| serde_json::to_vec(&value).expect("json"));
    let (status, text) = client.request(method, path, bytes).await.expect("request");
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

async fn hire_role(client: &SignedClient, role: &str) -> (u16, Value) {
    call(client, "POST", "/v1/hires", Some(json!({"role": role}))).await
}

fn id_of(body: &Value) -> String {
    body["hire"]["id"].as_str().expect("hire id").to_string()
}

fn roles_of(body: &Value) -> Vec<&str> {
    body["hires"]
        .as_array()
        .expect("hires")
        .iter()
        .map(|hire| hire["role"].as_str().expect("role"))
        .collect()
}

fn code_of(body: &Value) -> &str {
    body["detail"]["code"].as_str().expect("code")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hire_is_listed_renamed_and_dismissed() {
    let host = TestHost::start("hires-lifecycle").await;
    let client = host.paired_with("hirer", &[Permission::Approve]).await;

    let (status, created) = hire_role(&client, "bob").await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(created["hire"]["area"], "programacion");
    assert_eq!(created["hire"]["name"], "Bob");
    assert_eq!(created["hire"]["avatar"], "bob");
    assert_eq!(created["hire"]["status"], "hired");
    let id = id_of(&created);

    // The role already hired answers with that hire, unchanged.
    let (status, again) = call(
        &client,
        "POST",
        "/v1/hires",
        Some(json!({"role": "bob", "name": "Otro"})),
    )
    .await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(id_of(&again), id);
    assert_eq!(again["hire"]["name"], "Bob");

    let (status, listed) = call(&client, "GET", "/v1/hires", None).await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(roles_of(&listed), ["bob"]);

    let (status, renamed) = call(
        &client,
        "POST",
        &format!("/v1/hires/{id}/update"),
        Some(json!({"name": "Roberto", "avatar": "ines"})),
    )
    .await;
    assert_eq!(status, 200, "{renamed}");
    assert_eq!(renamed["hire"]["name"], "Roberto");
    assert_eq!(renamed["hire"]["avatar"], "ines");

    let dismiss = format!("/v1/hires/{id}/dismiss");
    let (status, dismissed) = call(&client, "POST", &dismiss, None).await;
    assert_eq!(status, 200, "{dismissed}");
    assert_eq!(dismissed["hire"]["status"], "removed");
    let (status, twice) = call(&client, "POST", &dismiss, None).await;
    assert_eq!(status, 200, "{twice}");
    assert_eq!(twice["hire"]["status"], "removed");

    let (_, active) = call(&client, "GET", "/v1/hires", None).await;
    assert!(roles_of(&active).is_empty(), "{active}");
    let (_, all) = call(&client, "GET", "/v1/hires?include_removed=true", None).await;
    assert_eq!(all["hires"][0]["status"], "removed", "{all}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_area_holds_eight_hires_and_a_dismissed_one_frees_its_place() {
    let host = TestHost::start("hires-limit").await;
    let client = host.paired_with("hirer", &[Permission::Approve]).await;
    let team = [
        "ernesto", "bob", "nicole", "ines", "hugo", "sami", "dani", "conan",
    ];
    let mut ernesto = String::new();
    for role in team {
        let (status, body) = hire_role(&client, role).await;
        assert_eq!(status, 201, "{role}: {body}");
        if role == "ernesto" {
            ernesto = id_of(&body);
        }
    }

    let (status, full) = hire_role(&client, "ravi").await;
    assert_eq!(status, 409, "{full}");
    assert_eq!(code_of(&full), "area_full");
    let (status, same) = hire_role(&client, "ines").await;
    assert_eq!(
        status, 200,
        "a role already hired is not counted again: {same}"
    );
    let (status, other) = hire_role(&client, "vera").await;
    assert_eq!(status, 201, "another area is not affected: {other}");

    let (status, _) = call(
        &client,
        "POST",
        &format!("/v1/hires/{ernesto}/dismiss"),
        None,
    )
    .await;
    assert_eq!(status, 200);
    let (status, freed) = hire_role(&client, "ravi").await;
    assert_eq!(status, 201, "the dismissed place is free: {freed}");
}

#[tokio::test(flavor = "multi_thread")]
async fn onboarding_contracts_the_level_once_and_the_roles_as_chosen() {
    let host = TestHost::start("onboarding-level").await;
    let client = host.paired_with("hirer", &[Permission::Approve]).await;

    let (status, before) = call(&client, "GET", "/v1/onboarding", None).await;
    assert_eq!(status, 200, "{before}");
    assert_eq!(before["done"], false);

    let body = json!({"areas": ["video"], "level": "personal"});
    let (status, first) = call(&client, "POST", "/v1/onboarding", Some(body.clone())).await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(first["done"], true);
    assert_eq!(roles_of(&first), ["vera"]);
    assert!(first["hires"][0]["avatar"].is_null(), "{first}");
    let (status, second) = call(&client, "POST", "/v1/onboarding", Some(body)).await;
    assert_eq!(status, 200, "{second}");
    assert_eq!(id_of_first(&second), id_of_first(&first));
    let (_, listed) = call(&client, "GET", "/v1/hires", None).await;
    assert_eq!(
        roles_of(&listed),
        ["vera"],
        "a second answer does not duplicate"
    );
    let (_, state) = call(&client, "GET", "/v1/onboarding", None).await;
    assert_eq!(state["done"], true);
    assert_eq!(state["areas"], json!(["video"]));
}

/// The first hire of an onboarding answer.
fn id_of_first(body: &Value) -> String {
    body["hires"][0]["id"]
        .as_str()
        .expect("hire id")
        .to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn onboarding_with_roles_contracts_exactly_those_roles() {
    let host = TestHost::start("onboarding-roles").await;
    let client = host.paired_with("hirer", &[Permission::Approve]).await;

    let body = json!({"areas": ["diseno_grafico"], "roles": ["kenji"]});
    let (status, chosen) = call(&client, "POST", "/v1/onboarding", Some(body)).await;
    assert_eq!(status, 200, "{chosen}");
    assert_eq!(roles_of(&chosen), ["kenji"], "the level is not used");

    let refused = json!({"areas": ["audio"], "roles": ["kenji"]});
    let (status, outside) = call(&client, "POST", "/v1/onboarding", Some(refused)).await;
    assert_eq!(status, 422, "{outside}");
    assert_eq!(outside["detail"]["field"], "roles");
    let (_, listed) = call(&client, "GET", "/v1/hires", None).await;
    assert_eq!(
        roles_of(&listed),
        ["kenji"],
        "a refused answer contracts nothing"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn onboarding_contracts_nothing_when_the_area_would_overflow() {
    let host = TestHost::start("onboarding-atomic").await;
    let client = host.paired_with("hirer", &[Permission::Approve]).await;
    let (status, _) = hire_role(&client, "ravi").await;
    assert_eq!(status, 201);

    let body = json!({"areas": ["programacion"], "level": "equipo"});
    let (status, refused) = call(&client, "POST", "/v1/onboarding", Some(body)).await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(code_of(&refused), "area_full");
    let (_, listed) = call(&client, "GET", "/v1/hires", None).await;
    assert_eq!(roles_of(&listed), ["ravi"], "none of the team was kept");
    let (_, state) = call(&client, "GET", "/v1/onboarding", None).await;
    assert_eq!(state["done"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn skipping_marks_the_onboarding_done_and_contracts_nobody() {
    let host = TestHost::start("onboarding-skip").await;
    let client = host.paired_with("hirer", &[Permission::Approve]).await;

    let skip = json!({"skip": true});
    let (status, skipped) = call(&client, "POST", "/v1/onboarding", Some(skip.clone())).await;
    assert_eq!(status, 200, "{skipped}");
    assert_eq!(skipped["done"], true);
    assert_eq!(skipped["hires"], json!([]));
    let (status, again) = call(&client, "POST", "/v1/onboarding", Some(skip)).await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(again["hires"], json!([]));

    let (_, state) = call(&client, "GET", "/v1/onboarding", None).await;
    assert_eq!(state["done"], true);
    assert_eq!(state["areas"], json!([]));
    let (_, listed) = call(&client, "GET", "/v1/hires", None).await;
    assert!(roles_of(&listed).is_empty(), "{listed}");
}

#[tokio::test(flavor = "multi_thread")]
async fn skipping_a_done_onboarding_keeps_its_hires() {
    let host = TestHost::start("onboarding-skip-kept").await;
    let client = host.paired_with("hirer", &[Permission::Approve]).await;
    let body = json!({"areas": ["video"], "level": "personal"});
    let (status, first) = call(&client, "POST", "/v1/onboarding", Some(body)).await;
    assert_eq!(status, 200, "{first}");

    let (status, skipped) = call(
        &client,
        "POST",
        "/v1/onboarding",
        Some(json!({"skip": true})),
    )
    .await;
    assert_eq!(status, 200, "{skipped}");
    assert_eq!(skipped["hires"], json!([]));
    let (_, listed) = call(&client, "GET", "/v1/hires", None).await;
    assert_eq!(roles_of(&listed), ["vera"], "skipping touched the team");
    let (_, state) = call(&client, "GET", "/v1/onboarding", None).await;
    assert_eq!(state["areas"], json!(["video"]));
}

#[tokio::test(flavor = "multi_thread")]
async fn skip_is_refused_with_a_selection_and_no_areas_is_still_refused() {
    let host = TestHost::start("onboarding-skip-refused").await;
    let client = host.paired_with("hirer", &[Permission::Approve]).await;

    for body in [
        json!({"skip": true, "areas": ["video"]}),
        json!({"skip": true, "roles": ["bob"]}),
    ] {
        let (status, refused) = call(&client, "POST", "/v1/onboarding", Some(body)).await;
        assert_eq!(status, 422, "{refused}");
        assert_eq!(code_of(&refused), "invalid_field");
        assert_eq!(refused["detail"]["field"], "skip");
    }
    let (status, empty) = call(
        &client,
        "POST",
        "/v1/onboarding",
        Some(json!({"skip": false, "areas": []})),
    )
    .await;
    assert_eq!(status, 422, "{empty}");
    assert_eq!(code_of(&empty), "invalid_field");
    let (_, state) = call(&client, "GET", "/v1/onboarding", None).await;
    assert_eq!(state["done"], false, "a refused answer marks nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_and_invalid_input_is_refused_with_a_code() {
    let host = TestHost::start("hires-invalid").await;
    let client = host.paired_with("hirer", &[Permission::Approve]).await;

    let (status, body) = hire_role(&client, "alfred").await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(code_of(&body), "unknown_role");
    assert_eq!(body["detail"]["field"], "role");

    let (status, body) = call(
        &client,
        "POST",
        "/v1/hires",
        Some(json!({"role": "bob", "avatar": "alfred"})),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["detail"]["field"], "avatar");

    for name in ["   ", "Ana\tB"] {
        let (status, body) = call(
            &client,
            "POST",
            "/v1/hires",
            Some(json!({"role": "bob", "name": name})),
        )
        .await;
        assert_eq!(status, 422, "{body}");
        assert_eq!(code_of(&body), "invalid_name");
    }

    let (status, body) = call(
        &client,
        "POST",
        "/v1/onboarding",
        Some(json!({"areas": ["nope"]})),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(code_of(&body), "unknown_area");
    let (status, body) = call(
        &client,
        "POST",
        "/v1/onboarding",
        Some(json!({"areas": []})),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    let (status, body) = call(
        &client,
        "POST",
        "/v1/onboarding",
        Some(json!({"areas": ["audio"], "roles": ["alfred"]})),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(code_of(&body), "unknown_role");

    let (status, body) = call(
        &client,
        "POST",
        "/v1/hires/hire_none/update",
        Some(json!({"name": "X"})),
    )
    .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(code_of(&body), "no_such_hire");
    let (status, body) = call(&client, "POST", "/v1/hires/hire_none/dismiss", None).await;
    assert_eq!(status, 404, "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_update_changes_nothing_and_a_dismissed_hire_is_not_renamed() {
    let host = TestHost::start("hires-update").await;
    let client = host.paired_with("hirer", &[Permission::Approve]).await;
    let (_, created) = hire_role(&client, "bob").await;
    let id = id_of(&created);

    let (status, body) = call(
        &client,
        "POST",
        &format!("/v1/hires/{id}/update"),
        Some(json!({"name": "Nuevo", "avatar": "alfred"})),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    let (_, listed) = call(&client, "GET", "/v1/hires", None).await;
    assert_eq!(
        listed["hires"][0]["name"], "Bob",
        "the name was not written"
    );

    call(&client, "POST", &format!("/v1/hires/{id}/dismiss"), None).await;
    let (status, body) = call(
        &client,
        "POST",
        &format!("/v1/hires/{id}/update"),
        Some(json!({"name": "Tarde"})),
    )
    .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(code_of(&body), "hire_not_active");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_avatar_is_kept_removed_or_set_apart_from_the_name() {
    let host = TestHost::start("hires-avatar").await;
    let client = host.paired_with("hirer", &[Permission::Approve]).await;
    let (_, created) = hire_role(&client, "bob").await;
    let id = id_of(&created);
    let update = format!("/v1/hires/{id}/update");

    let (status, renamed) = call(&client, "POST", &update, Some(json!({"name": "Roberto"}))).await;
    assert_eq!(status, 200, "{renamed}");
    assert_eq!(renamed["hire"]["avatar"], "bob", "an absent avatar stays");

    let (status, removed) = call(&client, "POST", &update, Some(json!({"avatar": null}))).await;
    assert_eq!(status, 200, "{removed}");
    assert!(removed["hire"]["avatar"].is_null(), "{removed}");
    assert_eq!(
        removed["hire"]["name"], "Roberto",
        "removing keeps the name"
    );

    let (status, set) = call(&client, "POST", &update, Some(json!({"avatar": "ines"}))).await;
    assert_eq!(status, 200, "{set}");
    assert_eq!(set["hire"]["avatar"], "ines");
    assert_eq!(set["hire"]["name"], "Roberto", "setting keeps the name");

    let (status, refused) = call(&client, "POST", &update, Some(json!({"avatar": "alfred"}))).await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(code_of(&refused), "invalid_field");
    assert_eq!(refused["detail"]["field"], "avatar");

    let (status, empty) = call(&client, "POST", &update, Some(json!({}))).await;
    assert_eq!(status, 422, "{empty}");
    assert_eq!(code_of(&empty), "invalid_field");

    let (_, listed) = call(&client, "GET", "/v1/hires", None).await;
    assert_eq!(
        listed["hires"][0]["avatar"], "ines",
        "refusals change nothing"
    );

    call(&client, "POST", &format!("/v1/hires/{id}/dismiss"), None).await;
    let (status, late) = call(&client, "POST", &update, Some(json!({"avatar": null}))).await;
    assert_eq!(status, 409, "{late}");
    assert_eq!(code_of(&late), "hire_not_active");
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_user_writes_the_team() {
    let host = TestHost::start("hires-authz").await;
    let reader = host.paired_with("reader", &[]).await;
    let agent = host.paired_with("agent", &[Permission::Respond]).await;

    for path in ["/v1/hires", "/v1/onboarding"] {
        let (status, body) = call(&reader, "GET", path, None).await;
        assert_eq!(status, 200, "{path}: {body}");
    }

    let writes = [
        ("POST", "/v1/hires", json!({"role": "bob"})),
        ("POST", "/v1/hires/hire_1/update", json!({"name": "X"})),
        ("POST", "/v1/hires/hire_1/update", json!({"avatar": null})),
        ("POST", "/v1/hires/hire_1/dismiss", json!({})),
        ("POST", "/v1/onboarding", json!({"areas": ["video"]})),
        ("POST", "/v1/onboarding", json!({"skip": true})),
    ];
    for client in [&reader, &agent] {
        for (method, path, body) in &writes {
            let (status, answer) = call(client, method, path, Some(body.clone())).await;
            assert_eq!(status, 403, "{path}: {answer}");
        }
    }

    // A request without a device signature (what a hook sends) is refused and
    // leaves the team as it was.
    let body = r#"{"role":"bob"}"#;
    let request = format!(
        "POST /v1/hires HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (status, _) = raw_exchange(host.addr, request.as_bytes()).await;
    assert_eq!(status, 401);
    let (_, listed) = call(&reader, "GET", "/v1/hires?include_removed=true", None).await;
    assert!(roles_of(&listed).is_empty(), "{listed}");
}

/// The sorted keys of a JSON object, so an extra field fails the test.
fn keys_of(body: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = body
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

/// `can_hire` reports the `APPROVE` grant the device already has: a reader gets
/// `false`, the user gets `true`, and no other permission data is shown.
#[tokio::test(flavor = "multi_thread")]
async fn can_hire_follows_the_approve_grant_and_nothing_else() {
    let host = TestHost::start("hires-can-hire").await;
    let reader = host.paired_with("reader", &[]).await;
    let hirer = host.paired_with("hirer", &[Permission::Approve]).await;

    for (client, expected) in [(&reader, false), (&hirer, true)] {
        let (status, state) = call(client, "GET", "/v1/onboarding", None).await;
        assert_eq!(status, 200, "{state}");
        assert_eq!(state["can_hire"], expected, "{state}");
        assert_eq!(keys_of(&state), ["areas", "can_hire", "done", "ok"]);

        let (status, listed) = call(client, "GET", "/v1/hires", None).await;
        assert_eq!(status, 200, "{listed}");
        assert_eq!(listed["can_hire"], expected, "{listed}");
        assert_eq!(keys_of(&listed), ["can_hire", "hires", "ok"]);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn contracting_registers_without_a_pane_or_a_seat() {
    let host = TestHost::start("hires-no-panes").await;
    let client = host.paired_with("hirer", &[Permission::Approve]).await;
    let (_, before) = call(&client, "GET", "/v1/chat/bootstrap", None).await;

    hire_role(&client, "bob").await;
    call(
        &client,
        "POST",
        "/v1/onboarding",
        Some(json!({"areas": ["video"], "level": "personal"})),
    )
    .await;

    let (_, after) = call(&client, "GET", "/v1/chat/bootstrap", None).await;
    assert_eq!(before, after, "contracting changed the chat state");
}
