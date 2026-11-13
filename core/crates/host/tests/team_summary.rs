//! `GET /v1/team/summary` through the real host (F1): one plain line per hired
//! professional, the same reading rules as the rest of the team, and nothing
//! for a request that is not signed.

mod harness;

use excubia_auth::Permission;
use excubia_chat::{
    action_digest, ApprovalKind, NewApproval, NewConversation, NewDelegation, NewPlan, NewProject,
    PlanStep, Touch, DEFAULT_TTL_SECONDS,
};
use excubia_state::{NewAgent, Status};
use excubia_test_support::SignedClient;
use harness::{raw_exchange, TestHost};
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

fn sorted_keys(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_team_is_an_empty_summary() {
    let host = TestHost::start("team-summary-empty").await;
    let client = host.paired_with("reader", &[]).await;

    let (status, summary) = call(&client, "GET", "/v1/team/summary", None).await;
    assert_eq!(status, 200, "{summary}");
    assert_eq!(summary, json!({"ok": true, "members": []}));
}

#[tokio::test(flavor = "multi_thread")]
async fn each_hire_gets_one_plain_line_with_the_fixed_keys() {
    let host = TestHost::start("team-summary-hired").await;
    let hirer = host.paired_with("hirer", &[Permission::Approve]).await;
    for role in ["bob", "vera"] {
        let (status, body) = call(&hirer, "POST", "/v1/hires", Some(json!({"role": role}))).await;
        assert_eq!(status, 201, "{body}");
    }

    let (status, summary) = call(&hirer, "GET", "/v1/team/summary", None).await;
    assert_eq!(status, 200, "{summary}");
    let members = summary["members"].as_array().expect("members");
    assert_eq!(members.len(), 2, "{summary}");
    for member in members {
        assert_eq!(
            sorted_keys(member),
            [
                "agent_id",
                "headline",
                "name",
                "role_title",
                "since",
                "status"
            ]
        );
        assert_eq!(member["status"], "idle", "{member}");
        assert_eq!(member["headline"], "Sin actividad", "{member}");
        assert!(member["role_title"].as_str().is_some_and(|t| !t.is_empty()));
    }
    let mut names: Vec<&str> = members
        .iter()
        .map(|member| member["name"].as_str().expect("name"))
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["Bob", "Vera"]);
}

/// The only member of the team, read through the route.
async fn only_member(client: &SignedClient) -> Value {
    let (status, summary) = call(client, "GET", "/v1/team/summary", None).await;
    assert_eq!(status, 200, "{summary}");
    let members = summary["members"].as_array().expect("members");
    assert_eq!(members.len(), 1, "{summary}");
    members[0].clone()
}

/// A panel of the state store in `status`, as the hooks of an agent would leave it.
async fn panel(host: &TestHost, pane_id: &str, status: Status) {
    host.store
        .add_agent(NewAgent {
            pane_id: pane_id.to_string(),
            workspace_id: "ws_1".to_string(),
            project: "excubia".to_string(),
            agent: "claude".to_string(),
            cwd: host.data_dir.to_string_lossy().to_string(),
            label: None,
            restored: false,
        })
        .await;
    host.store.set_status(pane_id, status).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hire_goes_from_idle_to_done_through_its_own_seat() {
    let host = TestHost::start("team-summary-flow").await;
    let (client, paired) = host.pair("Escritorio").await;
    for permission in [
        Permission::ReadState,
        Permission::Respond,
        Permission::Approve,
    ] {
        host.grant(&paired.device_id, permission);
    }
    let (status, body) = call(&client, "POST", "/v1/hires", Some(json!({"role": "bob"}))).await;
    assert_eq!(status, 201, "{body}");
    let hire_id = body["hire"]["id"].as_str().expect("hire").to_string();

    // The hire has no seat yet: it is idle until its first delegation.
    let chat = &host.chat;
    let project = chat
        .create_project(NewProject {
            name: "Demo".to_string(),
            root_path: host.data_dir.join("demo"),
        })
        .expect("project");
    let lead = project.lead.as_ref().expect("lead").agent_id.clone();
    let conversation = chat
        .create_conversation(NewConversation {
            title: Some("Trabajo".to_string()),
            project_id: Some(project.id.clone()),
        })
        .expect("conversation")
        .id;
    let member = only_member(&client).await;
    assert_eq!(member["status"], "idle", "{member}");
    assert_eq!(member["headline"], "Sin actividad", "{member}");

    // The first delegation gives the hire its seat in the project, in a panel.
    let seat = chat
        .seat_for_hire(&hire_id, &project.id)
        .expect("seat")
        .expect("a hire")
        .agent_id;
    chat.set_seat_pane(&seat, Some("pane_bob")).expect("pane");
    panel(&host, "pane_bob", Status::Idle).await;

    // Alfred hands the job to the lead, and the lead hands it to the hire.
    let from_alfred = chat
        .create_delegation(NewDelegation {
            conversation_id: conversation.clone(),
            from_agent_id: chat.alfred_id().expect("alfred"),
            to_agent_id: lead.clone(),
            task: "Revisa el presupuesto trimestral".to_string(),
            turn_key: "run_alfred_1".to_string(),
            parent_id: None,
            steps: 1,
            plan_id: None,
        })
        .expect("alfred delegation");
    let delegation = chat
        .create_delegation(NewDelegation {
            conversation_id: conversation.clone(),
            from_agent_id: lead,
            to_agent_id: seat.clone(),
            task: "Revisa el presupuesto trimestral".to_string(),
            turn_key: from_alfred.id.clone(),
            parent_id: Some(from_alfred.id),
            steps: 1,
            plan_id: None,
        })
        .expect("delegation");
    chat.start_delegation(&delegation.id).expect("start");
    let member = only_member(&client).await;
    assert_eq!(member["status"], "working", "{member}");
    assert_eq!(
        member["headline"], "Trabajando desde hace 0 min",
        "{member}"
    );

    // It asks to change a file: the person comes first, whatever the pane says.
    let args = json!({ "path": "presupuesto.md" });
    let cwd = host.data_dir.join("demo").to_string_lossy().to_string();
    let approval = chat
        .create_approval(NewApproval {
            conversation_id: Some(conversation),
            seat_id: Some(seat.clone()),
            agent_label: Some("bob".to_string()),
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Edit,
            tool: "edit".to_string(),
            summary: "Cambiar presupuesto.md".to_string(),
            cwd: Some(cwd.clone()),
            paths: Vec::new(),
            risk: None,
            action_digest: action_digest("edit", "edit", &args, &cwd, &[]),
            payload_redacted: Some(args),
            ttl_seconds: DEFAULT_TTL_SECONDS,
        })
        .expect("approval");
    let member = only_member(&client).await;
    assert_eq!(member["status"], "waiting_for_you", "{member}");
    assert_eq!(
        member["headline"], "Espera tu visto bueno para cambiar ficheros",
        "{member}"
    );

    // The person answers; the pane then reports a block.
    let decided = client
        .chat_decide_approval(&approval.id, "approve", "desktop-1")
        .await
        .expect("decide");
    assert_eq!(decided["approval"]["status"], "approved", "{decided}");
    panel(&host, "pane_bob", Status::Blocked).await;
    let member = only_member(&client).await;
    assert_eq!(member["status"], "blocked", "{member}");
    assert_eq!(member["headline"], "Necesita ayuda", "{member}");

    // The block clears and the delegation finishes.
    host.store.set_status("pane_bob", Status::Idle).await;
    chat.complete_delegation(&delegation.id, "Presupuesto revisado")
        .expect("complete");
    let member = only_member(&client).await;
    assert_eq!(member["status"], "done", "{member}");
    assert_eq!(member["headline"], "Terminó", "{member}");
    assert_eq!(member["agent_id"], hire_id.as_str());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unsigned_request_gets_no_summary() {
    let host = TestHost::start("team-summary-unsigned").await;
    let request = "GET /v1/team/summary HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let (status, _) = raw_exchange(host.addr, request.as_bytes()).await;
    assert_eq!(status, 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_proposed_plan_makes_its_hire_wait_for_you_and_shows_no_step_title() {
    let host = TestHost::start("team-summary-plan").await;
    let (client, paired) = host.pair("Escritorio").await;
    for permission in [
        Permission::ReadState,
        Permission::Respond,
        Permission::Approve,
    ] {
        host.grant(&paired.device_id, permission);
    }
    let (status, body) = call(&client, "POST", "/v1/hires", Some(json!({"role": "bob"}))).await;
    assert_eq!(status, 201, "{body}");
    let hire_id = body["hire"]["id"].as_str().expect("hire").to_string();

    let chat = &host.chat;
    let project = chat
        .create_project(NewProject {
            name: "Demo".to_string(),
            root_path: host.data_dir.join("demo"),
        })
        .expect("project");
    let conversation = chat
        .create_conversation(NewConversation {
            title: Some("Trabajo".to_string()),
            project_id: Some(project.id.clone()),
        })
        .expect("conversation")
        .id;
    let seat = chat
        .seat_for_hire(&hire_id, &project.id)
        .expect("seat")
        .expect("a hire")
        .agent_id;
    let steps = vec![
        PlanStep {
            title: "Borrar la base de datos".to_string(),
            touches: vec![Touch::Files],
        },
        PlanStep {
            title: "Enviar el informe".to_string(),
            touches: vec![Touch::Mail],
        },
        PlanStep {
            title: "Revisar".to_string(),
            touches: vec![Touch::Nothing],
        },
        PlanStep {
            title: "Cerrar".to_string(),
            touches: vec![Touch::Nothing],
        },
    ];
    chat.propose_plan(NewPlan {
        conversation_id: conversation,
        proposer_agent_id: seat,
        steps,
    })
    .expect("plan");

    let member = only_member(&client).await;
    assert_eq!(member["status"], "waiting_for_you", "{member}");
    assert_eq!(
        member["headline"], "Espera tu visto bueno para un plan de 4 pasos",
        "{member}"
    );
    assert!(!member.to_string().contains("Borrar"), "{member}");
}
