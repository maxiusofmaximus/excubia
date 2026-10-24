//! Pairing from the desktop: a link typed or pasted in, and the QR this
//! machine shows for a phone to read.

use std::sync::Arc;

use herdr_desktop_core::validate_device_label;
use herdr_desktop_core::{pair_through_link, PairLink};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::app_state::{lock, AppState};
use crate::commands::PairOutcome;
use crate::config::StoredPins;
use crate::lan;

type Shared = Arc<AppState>;

#[derive(Serialize)]
pub struct LanPairing {
    link: String,
    svg: String,
    expires_at: i64,
}

#[tauri::command]
pub async fn pair_link(
    app: AppHandle,
    state: State<'_, Shared>,
    link: String,
    label: String,
) -> Result<PairOutcome, String> {
    let shared = state.inner().clone();
    let identity = shared.settings()?.identity;
    let label = validate_device_label(&label).map_err(|error| error.to_string())?;
    let link = PairLink::parse_now(&link).map_err(|error| error.pairing_message())?;
    let paired = pair_through_link(&link, &identity.public_key_b64(), &label)
        .await
        .map_err(|error| error.pairing_message())?;

    let response = paired.response;
    let label = response.label.clone().unwrap_or(label);
    let mut identity = identity;
    identity.set_device_id(response.device_id.clone());
    identity.set_label(label.clone());
    shared.stop_local_host();
    shared.stop_stream();
    shared.update_identity(identity)?;
    {
        let mut inner = lock(&shared.inner);
        inner.config.local = false;
        inner.config.gateway_url = paired.base_url;
        inner.config.gateway_pins = Some(StoredPins::from(link.pins));
        inner.config.device_id = Some(response.device_id.clone());
        inner.config.label = label.clone();
        inner.config.paired = true;
    }
    shared.save_config()?;
    lock(&shared.store()).clear();
    shared.start_stream(app)?;

    Ok(PairOutcome {
        device_id: response.device_id,
        label,
        permissions: response.permissions,
        recovery_code: response.recovery_code.filter(|code| !code.is_empty()),
    })
}

/// Mints a single-use code and its LAN link, for the phone to scan.
#[tauri::command]
pub async fn lan_pairing(state: State<'_, Shared>) -> Result<LanPairing, String> {
    let shared = state.inner().clone();
    if !lock(&shared.inner).config.lan_pairing {
        return Err("Activa el emparejamiento por red local antes de mostrar el QR.".to_string());
    }
    let bind = lan::lan_bind().ok_or_else(|| {
        "No encuentro una dirección de red local privada en este equipo.".to_string()
    })?;
    let stdout = shared
        .run_host_cli(&["pair-code", "--lan", &bind.to_string()])
        .await?;
    let link = lan::pair_link_from(&stdout)?;
    let expires_at = PairLink::parse_now(&link)
        .map_err(|error| error.pairing_message())?
        .expires_at;
    let svg = lan::qr_svg(&link)?;
    Ok(LanPairing {
        link,
        svg,
        expires_at,
    })
}

/// Turns the LAN listener on or off. The local core restarts to apply it.
#[tauri::command]
pub fn set_lan_pairing(
    app: AppHandle,
    state: State<'_, Shared>,
    enabled: bool,
) -> Result<(), String> {
    let shared = state.inner().clone();
    let local = {
        let mut inner = lock(&shared.inner);
        inner.config.lan_pairing = enabled;
        inner.config.local
    };
    shared.save_config()?;
    if local {
        shared.start_local_host(app);
    }
    Ok(())
}
