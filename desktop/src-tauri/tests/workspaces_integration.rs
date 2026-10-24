//! Creating conversations and restored panes against the real compiled host.

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
    .expect("el emparejado contra el host real debe funcionar")
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn conversations_are_created_from_the_catalogue_and_panes_restore() {
    let Some(fake_agent) = fake_agent_binary() else {
        eprintln!("saltado: falta fake-agent.exe del núcleo");
        return;
    };
    let data_dir = std::env::temp_dir().join(format!("hn-ws-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).unwrap();

    // The host reads this catalogue; the client only ever names "fake".
    let command = fake_agent.display().to_string().replace('\\', "/");
    std::fs::write(
        data_dir.join("agents.toml"),
        format!("[agents.fake]\ncommand = \"{command}\"\nargs = [\"--delay-ms\", \"300\"]\n"),
    )
    .unwrap();

    let Some(mut host) = start_host(data_dir.clone(), vec![]).await else {
        eprintln!("saltado: falta el binario del núcleo (scripts/build-host.ps1)");
        return;
    };

    let identity = DeviceIdentity::generate("dev-integration", "integration");
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

    let catalog = client.catalog().await.expect("el catálogo del host");
    let fake = catalog
        .iter()
        .find(|agent| agent.name == "fake")
        .expect("fake debe estar en el catálogo");
    assert!(
        fake.available,
        "el ejecutable existe, luego está disponible"
    );

    let cwd = std::env::temp_dir().join(format!("hn-ws-cwd-{}", std::process::id()));
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd_text = cwd.display().to_string();

    let created = client
        .create_workspace("fake", &cwd_text, Some("Prueba de integración"))
        .await
        .expect("crear una conversación contra el host real debe funcionar");
    assert!(!created.pane_id.is_empty());

    // The new pane must appear in the state.
    let mut seen = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while tokio::time::Instant::now() < deadline && !seen {
        if let Ok(state) = client.state().await {
            seen = state
                .agents
                .iter()
                .any(|agent| agent.pane_id == created.pane_id);
        }
        if !seen {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    assert!(seen, "el panel creado debe aparecer en el estado");

    // An unknown name and a bad cwd are 422 with the field exposed.
    let unknown = client
        .create_workspace("ghost", &cwd_text, None)
        .await
        .unwrap_err();
    match &unknown {
        CoreError::Status {
            status: 422, field, ..
        } => assert_eq!(field.as_deref(), Some("agent")),
        other => panic!("esperaba 422 de agente, llegó {other:?}"),
    }
    assert!(unknown
        .describe(WriteAction::CreateWorkspace)
        .message
        .contains("catálogo"));

    let bad_cwd = client
        .create_workspace("fake", "D:/no-existe-hn-5", None)
        .await
        .unwrap_err();
    match &bad_cwd {
        CoreError::Status {
            status: 422, field, ..
        } => assert_eq!(field.as_deref(), Some("cwd")),
        other => panic!("esperaba 422 de cwd, llegó {other:?}"),
    }

    // A paired device without the grant is refused with 403.
    let second_identity = DeviceIdentity::generate("dev-second", "second");
    let second = pair(&host, &second_identity).await;
    let second_client = client_for(&host, &second_identity, &second.device_id);
    let denied = second_client
        .create_workspace("fake", &cwd_text, None)
        .await
        .unwrap_err();
    assert!(matches!(denied, CoreError::Forbidden));
    assert!(denied
        .describe(WriteAction::CreateWorkspace)
        .message
        .contains("RESPOND"));

    // Give the snapshot time to persist, then kill the host: the pane comes
    // back as `restored` and driving it is a 409.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let killed = std::process::Command::new("taskkill")
        .args(["/PID", &host.pid.to_string(), "/F"])
        .status()
        .expect("taskkill debe ejecutarse");
    assert!(killed.success());
    host.next_ready().await;

    let client = client_for(&host, &identity, &first.device_id);
    let mut restored = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while tokio::time::Instant::now() < deadline && restored.is_none() {
        if let Ok(state) = client.state().await {
            restored = state
                .agents
                .into_iter()
                .find(|agent| agent.pane_id == created.pane_id);
        }
        if restored.is_none() {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    let restored = restored.expect("el panel debe restaurarse tras reiniciar");
    assert!(restored.restored, "la fila debe llevar restored: true");

    let error = client.respond(&restored.pane_id, "hola").await.unwrap_err();
    match &error {
        CoreError::Status {
            status, message, ..
        } => {
            assert_eq!(*status, 409);
            assert!(message.contains("no running agent"), "{message}");
        }
        other => panic!("esperaba 409 al conducir un panel restaurado, llegó {other:?}"),
    }

    host.stop().await;
    let _ = std::fs::remove_dir_all(&cwd);
}
