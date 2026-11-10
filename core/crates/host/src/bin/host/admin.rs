//! The one-shot commands of `host`: pairing codes, the LAN certificate, the hook
//! token, the Claude settings, mail and the trust store. Each acts on the data
//! dir and exits; the daemon is `daemon.rs`.

use std::net::SocketAddr;
use std::path::Path;

use anyhow::{bail, Context, Result};
use excubia_core_auth::{DeviceStore, Permission};
use excubia_core_chat::{ChatStore, HookScope};
use excubia_core_host::cli::{DevicesCommand, TlsCommand};
use excubia_core_host::mail_cli::{self, MailInvocation};
use excubia_core_host::pake_cli;
use excubia_core_host::tls_slots::TlsSlots;
use excubia_core_host::{load_hook_token, paths};
use excubia_core_pin::{unix_now, PairingLink};

/// Mint a single-use pairing code. Only its hash is stored, so the plaintext
/// exists on this screen and nowhere else. With `--lan` it also prints the
/// pairing link: the code, the LAN certificate's pins and the addresses to
/// reach. The pins make a scanned device trust this host's certificate and no
/// other.
pub(super) fn pair_code(ttl_seconds: f64, lan: &[SocketAddr], data_dir: &Path) -> Result<()> {
    paths::ensure_data_dir(data_dir)?;
    let store = DeviceStore::open(Some(paths::devices_path(data_dir)));
    let code = store.issue_enrollment_code(ttl_seconds)?;
    println!("pairing code: {code}");
    println!("single use, valid for {ttl_seconds:.0} seconds; enter it on the device");
    if !lan.is_empty() {
        let link = PairingLink {
            code,
            pins: TlsSlots::open(data_dir).pins()?,
            addresses: lan.to_vec(),
            expires_at: unix_now() + ttl_seconds.round() as i64,
        };
        println!("pairing link: {}", link.to_uri());
    }
    println!("data dir: {}", data_dir.display());
    Ok(())
}

/// Ask the running daemon for its PAKE pairing code and print it, once. The code lives only in the daemon's
/// memory, so this process cannot make it: it calls the daemon's loopback admin route with the host hook token.
/// With `uri` it also prints the v2 pairing link (the QR payload, no address).
pub(super) async fn pair_code_pake(port: u16, uri: bool, data_dir: &Path) -> Result<()> {
    let token = pake_cli::read_hook_token(data_dir)?;
    let armed = pake_cli::arm_code(SocketAddr::from(([127, 0, 0, 1], port)), &token).await?;
    // Checked before printing: a code shown without the link that was asked for would be half an answer.
    let link = match (uri, &armed.uri) {
        (true, None) => {
            bail!("the daemon is older than F3a and returns no pairing uri: restart it")
        }
        (true, Some(link)) => Some(link),
        (false, _) => None,
    };
    println!(
        "código de emparejamiento: {}",
        pake_cli::grouped(&armed.code)
    );
    println!(
        "válido {} s, un solo intento (se gasta con el primer intento, acierte o no); escríbelo en el dispositivo",
        armed.expires_in
    );
    if let Some(link) = link {
        // The link holds the live code: a secret like the code, for whoever puts it on screen.
        println!("pairing uri: {link}");
    }
    println!("daemon: 127.0.0.1:{port}; data dir: {}", data_dir.display());
    Ok(())
}

/// Prepare or activate the LAN certificate (`tls_slots`). A device can only
/// accept the backup pin if it was given it, and it is given it only by a link
/// printed while that pin is prepared. Rotation therefore reaches a device
/// that paired or re-paired in that window; the daemon must restart to serve
/// the new certificate.
pub(super) fn tls(command: TlsCommand, data_dir: &Path) -> Result<()> {
    paths::ensure_data_dir(data_dir)?;
    let slots = TlsSlots::open(data_dir);
    match command {
        TlsCommand::Prepare => {
            let pin = slots.prepare()?;
            println!("prepared pin: {pin}");
            println!("links printed from now on carry it as the backup pin");
        }
        TlsCommand::Activate => {
            let (active, previous) = slots.activate()?;
            println!("active pin: {active}");
            println!(
                "previous pin: {previous}; links printed from now on carry it as the backup pin"
            );
            println!("restart the daemon so the LAN listener serves the new certificate");
        }
    }
    println!("data dir: {}", data_dir.display());
    Ok(())
}

/// Print (or write) the Claude settings the seat gives each run: the same bytes
/// `launch` writes, so a harness that runs Claude by hand cannot drift from it.
pub(super) fn claude_settings(hook: &Path, out: Option<&Path>) -> Result<()> {
    let bytes = excubia_core_seat::claude_settings(hook).context("encode the Claude settings")?;
    match out {
        Some(path) => {
            std::fs::write(path, bytes).with_context(|| format!("write {}", path.display()))?
        }
        None => println!("{}", String::from_utf8_lossy(&bytes)),
    }
    Ok(())
}

/// Print the token agent hooks authenticate with. It lives in the data
/// directory; the daemon hands it to each agent it starts. With `--seat`, it
/// mints a seat token instead: it covers the seat's approvals (what a resident
/// panel carries), and it is stored hashed and revoked by the seat's lifecycle.
pub(super) fn hook_token(seat: Option<&str>, data_dir: &Path) -> Result<()> {
    paths::ensure_data_dir(data_dir)?;
    let token = match seat {
        None => load_hook_token(data_dir)?,
        Some(agent_id) => {
            let chat = ChatStore::open(&paths::chat_db_path(data_dir))
                .context("open the chat database")?;
            chat.seat(agent_id)
                .with_context(|| format!("no such seat: {agent_id}"))?;
            chat.issue_hook_token(&HookScope::seat(agent_id))?
        }
    };
    println!("hook token: {token}");
    println!("sent as the X-Excubia-Hook-Token header to POST /v1/hooks/report (loopback only)");
    println!("spawned agents receive it in EXCUBIA_HOOK_TOKEN");
    println!("data dir: {}", data_dir.display());
    Ok(())
}

/// `host mail ...`: the errors name their stable code, never a secret.
pub(super) async fn mail(invocation: MailInvocation) -> Result<()> {
    let mut out = std::io::stdout();
    mail_cli::main(invocation, &mut out)
        .await
        .map_err(|error| anyhow::anyhow!("{error} [{}]", error.code()))
}

pub(super) fn devices(command: DevicesCommand, data_dir: &Path) -> Result<()> {
    paths::ensure_data_dir(data_dir)?;
    let store = DeviceStore::open(Some(paths::devices_path(data_dir)));
    match command {
        DevicesCommand::List => {
            let devices = store.list();
            if devices.is_empty() {
                println!("no devices paired (data dir: {})", data_dir.display());
            }
            let pin = TlsSlots::open(data_dir).active_pin().ok();
            for device in devices {
                let label = device.label.as_deref().unwrap_or("-");
                let revoked = if device.revoked { " REVOKED" } else { "" };
                let permissions: Vec<&str> = device
                    .permissions
                    .iter()
                    .map(|permission| permission.label())
                    .collect();
                println!(
                    "{}  {}  {}  [{}]{}",
                    device.device_id,
                    device.curve.label(),
                    label,
                    permissions.join(","),
                    revoked
                );
                if device.permissions.is_empty() && !device.revoked {
                    if let Some(text) = excubia_core_api::device_sas(pin.as_ref(), &device) {
                        println!("    cadena de verificación: {text}");
                    }
                }
            }
            Ok(())
        }
        DevicesCommand::Revoke { device_id } => {
            if store.revoke(&device_id)? {
                println!("revoked {device_id}");
                Ok(())
            } else {
                bail!("no such device: {device_id}")
            }
        }
        DevicesCommand::Grant {
            device_id,
            permission,
        } => {
            let permission = Permission::from_label(&permission)
                .with_context(|| format!("unknown permission: {permission}"))?;
            if store.grant(&device_id, permission)? {
                println!("granted {} to {device_id}", permission.label());
                Ok(())
            } else {
                bail!("no such device: {device_id}")
            }
        }
        DevicesCommand::Recovery { device_id } => {
            let device = store
                .get(&device_id)
                .with_context(|| format!("no such device: {device_id}"))?;
            if device.revoked {
                bail!("device {device_id} is revoked: no recovery code is issued for it");
            }
            let code = store
                .issue_recovery_code(&device_id)?
                .with_context(|| format!("no such device: {device_id}"))?;
            println!("recovery code: {code}");
            println!("shown once; it replaces the previous recovery code of {device_id}");
            Ok(())
        }
    }
}
