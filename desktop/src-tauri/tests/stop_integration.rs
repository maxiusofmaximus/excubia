//! Catalogue and stop against the real compiled host.

mod common;

use std::time::Duration;

use common::{fake_agent_binary, run_cli, start_host};
use herdr_desktop_core::identity::DeviceIdentity;
use herdr_desktop_core::{CoreError, GatewayClient, WriteAction};
use herdr_notify_desktop::app_state::pair_code_from;

async fn pair(
    host: &common::RunningHost,
    identity: &DeviceIdentity,
) -> herdr_desktop_core::PairResponse {
    let code = pair_code_from(&run_cli(&host.binary, &host.data_dir, &["pair-code"])).unwrap();
    herdr_desktop_core::client::pair_device(
        &host.url(),
        &herdr_desktop_core::Transport::Plain,
        "p256",
        &identity.public_key_b64(),
        "integration",
        &code,
    )
    .await
    .expect("pairing")
}

fn client_for(
    host: &common::RunningHost,
    identity: &DeviceIdentity,
    device_id: &str,
) -> GatewayClient {
    let mut identity = identity.clone();
    identity.set_device_id(device_id);
    GatewayClient::new(&host.url(), identity).unwrap()
}

async fn wait_until_absent(client: &GatewayClient, pane_id: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while tokio::time::Instant::now() < deadline {
        if let Ok(state) = client.state().await {
            if !state.agents.iter().any(|agent| agent.pane_id == pane_id) {
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("el panel {pane_id} no salió del estado");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_catalogue_hides_missing_agents_and_stop_removes_the_pane() {
    let Some(fake_agent) = fake_agent_binary() else {
        eprintln!("saltado: falta fake-agent.exe del núcleo");
        return;
    };
    let data_dir = std::env::temp_dir().join(format!("hn-stop-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).unwrap();
    let command = fake_agent.display().to_string().replace('\\', "/");
    std::fs::write(
        data_dir.join("agents.toml"),
        format!(
            "[agents.fake]\ncommand = \"{command}\"\nargs = [\"--delay-ms\", \"300\"]\n\n[agents.missing]\ncommand = \"missing-agent\"\nargs = []\n"
        ),
    )
    .unwrap();

    let Some(mut host) = start_host(data_dir.clone(), vec![]).await else {
        eprintln!("saltado: falta el binario del núcleo (scripts/build-host.ps1)");
        return;
    };

    let identity = DeviceIdentity::generate("dev-stop", "stop");
    let first = pair(&host, &identity).await;
    run_cli(
        &host.binary,
        &host.data_dir,
        &[
            "devices",
            "grant",
            &first.device_id,
            "--permission",
            "respond",
        ],
    );
    let client = client_for(&host, &identity, &first.device_id);

    let catalog = client.catalog().await.expect("catálogo");
    let fake = catalog
        .iter()
        .find(|agent| agent.name == "fake")
        .expect("fake");
    assert!(fake.available);
    let missing = catalog
        .iter()
        .find(|agent| agent.name == "missing")
        .expect("missing");
    assert!(
        !missing.available,
        "un comando inexistente no está disponible"
    );

    let cwd = std::env::temp_dir().join(format!("hn-stop-cwd-{}", std::process::id()));
    std::fs::create_dir_all(&cwd).unwrap();
    let created = client
        .create_workspace("fake", &cwd.display().to_string(), None)
        .await
        .expect("crear");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let mut live = false;
    while tokio::time::Instant::now() < deadline && !live {
        if let Ok(state) = client.state().await {
            live = state
                .agents
                .iter()
                .any(|agent| agent.pane_id == created.pane_id);
        }
        if !live {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    assert!(live, "el panel creado debe existir antes de pararlo");

    client
        .stop_agent(&created.pane_id)
        .await
        .expect("parar un panel vivo");
    wait_until_absent(&client, &created.pane_id).await;

    // A second stop is pane_gone, not an idempotent success.
    let second = client.stop_agent(&created.pane_id).await.unwrap_err();
    match &second {
        CoreError::Status { status, code, .. } => {
            assert_eq!(*status, 400);
            assert_eq!(code.as_deref(), Some("pane_gone"));
        }
        other => panic!("esperaba pane_gone, llegó {other:?}"),
    }

    // A restored pane has no process: 409, and stop names RESPOND when the
    // device has no grant.
    let live_again = client
        .create_workspace("fake", &cwd.display().to_string(), Some("restaurado"))
        .await
        .expect("crear el segundo");
    tokio::time::sleep(Duration::from_secs(3)).await;
    let killed = std::process::Command::new("taskkill")
        .args(["/PID", &host.pid.to_string(), "/F"])
        .status()
        .expect("taskkill");
    assert!(killed.success());
    host.next_ready().await;
    let client = client_for(&host, &identity, &first.device_id);
    let error = client
        .stop_agent(&live_again.pane_id)
        .await
        .expect_err("un panel restaurado no se puede parar");
    match &error {
        CoreError::Status { status, .. } => assert_eq!(*status, 409),
        other => panic!("esperaba 409, llegó {other:?}"),
    }
    assert!(error
        .describe(WriteAction::Stop)
        .message
        .contains("No se puede detener"));

    let second_identity = DeviceIdentity::generate("dev-stop-denied", "denied");
    let second_device = pair(&host, &second_identity).await;
    let denied = client_for(&host, &second_identity, &second_device.device_id)
        .stop_agent(&live_again.pane_id)
        .await
        .unwrap_err();
    assert!(matches!(denied, CoreError::Forbidden));
    assert!(denied
        .describe(WriteAction::Stop)
        .action
        .as_deref()
        .unwrap()
        .contains("host devices grant <device_id>"));

    host.stop().await;
    let _ = std::fs::remove_dir_all(&cwd);
}
