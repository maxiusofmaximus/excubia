//! Real integration: the compiled core daemon, a fake agent of the core, an
//! automatic pairing, a response, and the supervisor restarting a killed child.
//!
//! Skips (without failing) when the binaries are not built: run
//! `scripts/build-host.ps1` and build `hn-core-test-support` first.

mod common;

use std::time::Duration;

use common::{fake_agent_binary, run_cli, start_host};
use herdr_desktop_core::identity::DeviceIdentity;
use herdr_desktop_core::GatewayClient;
use herdr_notify_desktop::app_state::pair_code_from;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_real_host_pairs_answers_and_survives_a_kill() {
    let Some(fake_agent) = fake_agent_binary() else {
        eprintln!("saltado: falta fake-agent.exe del núcleo");
        return;
    };
    let data_dir = std::env::temp_dir().join(format!("hn-host-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let Some(mut host) = start_host(
        data_dir,
        vec![format!("{} --delay-ms 300", fake_agent.display())],
    )
    .await
    else {
        eprintln!("saltado: falta el binario del núcleo (scripts/build-host.ps1)");
        return;
    };

    // Automatic pairing: mint a code with the CLI and pair the real client.
    let identity = DeviceIdentity::generate("dev-integration", "integration");
    let code = pair_code_from(&run_cli(&host.binary, &host.data_dir, &["pair-code"])).unwrap();
    let response = herdr_desktop_core::client::pair_device(
        &host.url(),
        &herdr_desktop_core::Transport::Plain,
        "p256",
        &identity.public_key_b64(),
        "integration",
        &code,
    )
    .await
    .expect("el emparejado contra el host real debe funcionar");
    run_cli(
        &host.binary,
        &host.data_dir,
        &[
            "devices",
            "grant",
            &response.device_id,
            "--permission",
            "respond",
        ],
    );

    // The fake agent must show up in the state.
    let mut identity = identity;
    identity.set_device_id(response.device_id.clone());
    let client = GatewayClient::new(&host.url(), identity).unwrap();
    let mut pane_id = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while tokio::time::Instant::now() < deadline && pane_id.is_none() {
        if let Ok(state) = client.state().await {
            pane_id = state.agents.first().map(|agent| agent.pane_id.clone());
        }
        if pane_id.is_none() {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    let pane_id = pane_id.expect("el agente falso debe aparecer en el estado");

    // And answering it must reach the core.
    client
        .respond(&pane_id, "hola desde la integración")
        .await
        .expect("responder contra el host real debe funcionar");

    // Kill the child: the supervisor must bring it back (new port, same data).
    let old_port = host.port;
    let killed = std::process::Command::new("taskkill")
        .args(["/PID", &host.pid.to_string(), "/F"])
        .status()
        .expect("taskkill debe ejecutarse");
    assert!(killed.success());
    let (new_port, _) = host.next_ready().await;
    assert_ne!(
        new_port, old_port,
        "el reinicio debe escuchar en un puerto nuevo"
    );

    host.stop().await;
}
