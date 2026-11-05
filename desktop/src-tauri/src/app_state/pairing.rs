use std::sync::Arc;

use herdr_desktop_core::client::pair_device;
use herdr_desktop_core::{CoreError, HostClient, Transport, WriteAction};
use tauri::{AppHandle, Emitter};

use super::{lock, AppState};

/// The desktop owns its local host (same user, loopback), so it grants itself
/// all of them; the "no ADMIN" policy is for phones only.
const LOCAL_PERMISSIONS: &[&str] = &["read_state", "respond", "send_key", "approve", "admin"];

impl AppState {
    /// Pairs this device against the managed host if it does not know it yet,
    /// then grants the permissions the UI needs. Both are host-side acts and
    /// the app is the host here.
    pub(super) async fn ensure_local_pairing(
        self: &Arc<Self>,
        app: &AppHandle,
        base_url: &str,
    ) -> Result<(), String> {
        let identity = lock(&self.inner)
            .identity
            .clone()
            .ok_or_else(|| "No hay identidad de dispositivo disponible.".to_string())?;
        let client =
            HostClient::new(base_url, identity.clone()).map_err(|error| error.to_string())?;
        let mut state = client.state().await;
        let paired = lock(&self.inner).config.paired;
        if paired && matches!(state, Err(CoreError::Forbidden)) {
            // A device paired before the grants existed: heal it once, then retry.
            self.grant_local_permissions(identity.device_id()).await?;
            state = client.state().await;
        }
        match state {
            Ok(state) => {
                if let Some(state) = lock(&self.store()).apply(state) {
                    let _ = app.emit("hn-state", state);
                }
                return Ok(());
            }
            Err(CoreError::Unauthorized) => {}
            Err(error) => return Err(error.describe(WriteAction::Read).text()),
        }

        let code = pair_code_from(&self.run_host_cli(&["pair-code"]).await?)?;
        let label = lock(&self.inner).config.label.clone();
        let response = pair_device(
            base_url,
            &Transport::Plain,
            "p256",
            &identity.public_key_b64(),
            &label,
            &code,
        )
        .await
        .map_err(|error| error.to_string())?;

        let mut updated = identity;
        updated.set_device_id(response.device_id.clone());
        updated.set_label(label);
        self.update_identity(updated)?;
        {
            let mut inner = lock(&self.inner);
            inner.config.device_id = Some(response.device_id.clone());
            inner.config.paired = true;
        }
        self.save_config()?;

        self.grant_local_permissions(&response.device_id).await
    }

    /// One `devices grant` per permission: a repeated `--permission` keeps only the last.
    async fn grant_local_permissions(&self, device_id: &str) -> Result<(), String> {
        for &permission in LOCAL_PERMISSIONS {
            self.run_host_cli(&["devices", "grant", device_id, "--permission", permission])
                .await?;
        }
        Ok(())
    }

    /// Runs the host CLI against the same data dir the daemon uses.
    pub async fn run_host_cli(&self, args: &[&str]) -> Result<String, String> {
        let config = lock(&self.host_config)
            .clone()
            .ok_or_else(|| "el host local no está configurado".to_string())?;
        let output = tokio::process::Command::new(&config.binary)
            .args(args)
            .arg("--data-dir")
            .arg(&config.data_dir)
            .output()
            .await
            .map_err(|error| format!("no se pudo ejecutar el host: {error}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "host {}: {}",
                args.first().copied().unwrap_or(""),
                stderr.trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

/// `pairing code: ABC123` → `ABC123`.
pub fn pair_code_from(stdout: &str) -> Result<String, String> {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("pairing code: "))
        .map(|code| code.trim().to_string())
        .filter(|code| !code.is_empty())
        .ok_or_else(|| "el host no devolvió un código de emparejamiento".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_local_device_is_granted_all_five_permissions() {
        assert_eq!(
            LOCAL_PERMISSIONS,
            &["read_state", "respond", "send_key", "approve", "admin"]
        );
    }

    #[test]
    fn the_pair_code_line_is_parsed() {
        let stdout = "pairing code: RANDTIA\nsingle use, valid for 600 seconds\n";
        assert_eq!(pair_code_from(stdout).unwrap(), "RANDTIA");
        assert!(pair_code_from("no devices paired").is_err());
    }
}
