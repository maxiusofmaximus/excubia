use super::*;
use hn_core_test_dir::TestDir;

#[test]
fn the_child_environment_never_names_a_key() {
    assert!(!CHILD_ENV.iter().any(|name| name.contains("KEY")));
    assert!(!CHILD_ENV.contains(&"STRATA_API_KEY"));
}

#[test]
fn health_reads_the_http10_answer_strata_sends() {
    let response = "HTTP/1.0 200 OK\r\nServer: BaseHTTP/0.6 Python/3.12\r\n\
                    Content-Type: application/json\r\nConnection: close\r\n\r\n\
                    {\"status\": \"ok\", \"max_context\": 65536, \"loaded\": true, \"api_key\": false}";
    let body = parse_health(response).expect("a 200 is a health answer");
    assert!(body["loaded"] == true);
    assert!(parse_health(&response.replacen("200 OK", "503 Busy", 1)).is_none());
    assert!(parse_health("HTTP/1.1 200 OK\r\n\r\nnot json").is_none());
}

#[test]
fn launch_status_reads_the_json_line_and_ignores_the_text_after_it() {
    let output = "{\"schema_version\": 1, \"ok\": true, \"running\": [{\"name\": \"q2_0-64k\", \
                  \"profile\": \"launch-q2_0-64k\", \"state\": \"ready\", \"port\": 8080, \
                  \"url\": \"http://127.0.0.1:8080\"}]}\n\
                  q2_0-64k: ready at http://127.0.0.1:8080\n";
    let running = launch_running(output).expect("the JSON line");
    assert_eq!(running.len(), 1);
    assert!(running[0]["state"] == "ready");

    let none =
        launch_running("{\"schema_version\": 1, \"ok\": true, \"running\": []}\n").expect("empty");
    assert!(none.is_empty());
    assert!(launch_running("no json here\n").is_err());
}

#[test]
fn the_model_needs_its_memory_and_says_how_much() {
    assert!(ram_ok(MIN_FREE_RAM - 1).is_err());
    assert!(ram_ok(MIN_FREE_RAM).is_ok());
    let error = ram_ok(GIB).expect_err("1 GiB is not enough");
    assert!(error.to_string().contains("38 GiB"), "{error}");
}

#[test]
fn the_agent_entry_parses_and_carries_the_local_endpoint() {
    let catalog = AgentCatalog::from_toml(&agent_block()).expect("entry");
    let spec = catalog.resolve(AGENT_NAME).expect("claude-local");
    assert_eq!(spec.command, "claude");
    assert_eq!(spec.env.get("ANTHROPIC_BASE_URL"), Some(&endpoint()));
    assert_eq!(
        spec.env.get("ANTHROPIC_MODEL").map(String::as_str),
        Some(MODEL)
    );
}

#[test]
fn enable_agent_writes_the_entry_once_and_keeps_the_rest() {
    let dir = TestDir::new("strata-enable");
    assert!(enable_agent(&dir).expect("first"));
    assert!(!enable_agent(&dir).expect("second"));
    let catalog = agents::load_or_create(&dir).expect("load");
    assert!(catalog.names().iter().any(|name| name == "claude"));
    assert!(catalog.names().iter().any(|name| name == AGENT_NAME));
}
