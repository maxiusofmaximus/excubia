//! «Mis dispositivos»: the devices the host trusts, read and revoked over
//! `/v1/devices`. Both need `ADMIN`, which only the operator's desktop holds. The
//! list carries public material only: never a key, a token or a push endpoint.

use reqwest::{Method, RequestBuilder};
use serde::de::{DeserializeOwned, IgnoredAny};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::client::{expect_json, validate_device_id, HostClient};
use crate::error::CoreError;

const DEVICES: &str = "/v1/devices";

/// One paired device, as the host lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Device {
    pub device_id: String,
    /// `null` for a device enrolled without a name: a null here must not fail the whole list.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub created_at: f64,
    /// Seconds since the epoch; `None` for a device that never signed a request.
    #[serde(default)]
    pub last_seen_at: Option<f64>,
    #[serde(default)]
    pub revoked: bool,
    #[serde(default)]
    pub permissions: Vec<String>,
    /// The six-digit verification string of a pending device, when the host has a pin.
    #[serde(default)]
    pub sas: Option<String>,
}

/// The devices, and the id this app signs as: the row the screen marks «Este equipo».
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeviceList {
    pub this_device: String,
    pub devices: Vec<Device>,
}

#[derive(Debug, Deserialize)]
struct DevicesResponse {
    devices: Vec<Device>,
}

/// A failure of the devices screen, by what the operator can do about it. The
/// screen words each kind; the core keeps only the status of a refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DeviceFailure {
    Offline,
    /// The host no longer trusts this app's device.
    Unauthorized,
    /// This device lacks `ADMIN`.
    Forbidden,
    NotFound,
    /// A grant to a device that already has permissions.
    NotPending,
    Refused {
        status: u16,
    },
    Protocol,
}

pub type DeviceResult<T> = Result<T, DeviceFailure>;

impl DeviceFailure {
    pub fn from_core(error: CoreError) -> Self {
        match error {
            CoreError::Unauthorized => Self::Unauthorized,
            CoreError::Forbidden => Self::Forbidden,
            CoreError::Transport(_) => Self::Offline,
            CoreError::Status { status: 404, .. } => Self::NotFound,
            CoreError::Status { status, .. } => Self::Refused { status },
            _ => Self::Protocol,
        }
    }
}

impl HostClient {
    /// Every device the host knows, revoked ones included.
    pub async fn devices(&self) -> DeviceResult<DeviceList> {
        let response: DevicesResponse = self.device_call(Method::GET, DEVICES).await?;
        Ok(DeviceList {
            this_device: self.device_id().to_string(),
            devices: response.devices,
        })
    }

    /// Marks a device revoked: it is refused from then on, and it stays listed.
    /// The id is checked before it becomes part of a path.
    pub async fn device_revoke(&self, device_id: &str) -> DeviceResult<()> {
        let device_id = device_id.trim();
        validate_device_id(device_id).map_err(DeviceFailure::from_core)?;
        let path = format!("{DEVICES}/{device_id}/revoke");
        let _: IgnoredAny = self.device_call(Method::POST, &path).await?;
        Ok(())
    }

    /// Gives a pending device its first permissions. `admin` is never sent: the host
    /// CLI keeps it, and an empty list is refused before anything leaves.
    pub async fn device_grant(&self, device_id: &str, permissions: &[String]) -> DeviceResult<()> {
        let device_id = device_id.trim();
        validate_device_id(device_id).map_err(DeviceFailure::from_core)?;
        if permissions.is_empty()
            || permissions
                .iter()
                .any(|id| id.eq_ignore_ascii_case("admin"))
        {
            return Err(DeviceFailure::Protocol);
        }
        let body = serde_json::to_vec(&json!({ "permissions": permissions }))
            .map_err(|_| DeviceFailure::Protocol)?;
        let path = format!("{DEVICES}/{device_id}/grant");
        let request = self.device_request(Method::POST, &path, Some(&body))?;
        let _: IgnoredAny = self.device_send(request).await.map_err(not_pending)?;
        Ok(())
    }

    async fn device_call<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
    ) -> DeviceResult<T> {
        self.device_send(self.device_request(method, path, None)?)
            .await
    }

    async fn device_send<T: DeserializeOwned>(&self, request: RequestBuilder) -> DeviceResult<T> {
        let response = request
            .send()
            .await
            .map_err(|error| DeviceFailure::from_core(CoreError::from(error)))?;
        expect_json(response)
            .await
            .map_err(DeviceFailure::from_core)
    }

    /// The calls here carry a body only when they have one: the signature covers its digest.
    fn device_request(
        &self,
        method: Method,
        path: &str,
        body: Option<&[u8]>,
    ) -> DeviceResult<RequestBuilder> {
        self.signed_call(method, path, &[], body)
            .map_err(DeviceFailure::from_core)
    }
}

/// The host answers `409` to a grant of a device that already has permissions.
fn not_pending(failure: DeviceFailure) -> DeviceFailure {
    match failure {
        DeviceFailure::Refused { status: 409 } => DeviceFailure::NotPending,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
    use reqwest::Request;
    use serde_json::json;

    use super::*;
    use crate::identity::DeviceIdentity;
    use crate::signing;

    fn client() -> (HostClient, VerifyingKey) {
        let identity = DeviceIdentity::generate("dev-desk", "Escritorio");
        let key = identity.verifying_key();
        let client = HostClient::new("https://127.0.0.1:8443", identity).unwrap();
        (client, key)
    }

    /// The headers must verify against the canonical string of the request as it
    /// leaves: method, path, body digest, timestamp and nonce.
    fn assert_signed(request: &Request, key: &VerifyingKey, body: &[u8]) {
        let header = |name: &str| request.headers()[name].to_str().unwrap().to_string();
        assert_eq!(header("X-HN-Device"), "dev-desk");
        let canonical = signing::canonical_string(
            request.method().as_str(),
            request.url().path(),
            body,
            header("X-HN-Timestamp").parse().unwrap(),
            &header("X-HN-Nonce"),
        );
        let signature =
            Signature::from_der(&STANDARD.decode(header("X-HN-Signature")).unwrap()).unwrap();
        key.verify(canonical.as_bytes(), &signature).unwrap();
    }

    fn status(status: u16) -> CoreError {
        CoreError::Status {
            status,
            code: None,
            field: None,
            message: "respuesta".to_string(),
        }
    }

    #[test]
    fn the_list_is_a_signed_get_with_no_body() {
        let (client, key) = client();
        let request = client
            .device_request(Method::GET, DEVICES, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request.method(), Method::GET);
        assert_eq!(request.url().path(), "/v1/devices");
        assert!(request.body().is_none());
        assert_signed(&request, &key, b"");
    }

    #[test]
    fn a_revoke_is_a_signed_post_to_that_device() {
        let (client, key) = client();
        let path = "/v1/devices/dev-abc123/revoke";
        let request = client
            .device_request(Method::POST, path, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request.method(), Method::POST);
        assert_eq!(request.url().path(), path);
        assert!(request.body().is_none());
        assert_signed(&request, &key, b"");
    }

    #[test]
    fn a_grant_is_a_signed_post_with_its_permissions_as_the_body() {
        let (client, key) = client();
        let path = "/v1/devices/dev-abc123/grant";
        let body = br#"{"permissions":["read_state"]}"#;
        let request = client
            .device_request(Method::POST, path, Some(body))
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request.method(), Method::POST);
        assert_eq!(request.url().path(), path);
        assert_eq!(request.body().unwrap().as_bytes().unwrap(), body);
        assert_signed(&request, &key, body);
    }

    #[tokio::test]
    async fn an_id_that_is_not_a_device_id_never_reaches_the_path() {
        let (client, _) = client();
        assert_eq!(
            client.device_revoke("../devices").await,
            Err(DeviceFailure::Protocol)
        );
        assert_eq!(
            client.device_revoke("   ").await,
            Err(DeviceFailure::Protocol)
        );
        let read_state = ["read_state".to_string()];
        assert_eq!(
            client.device_grant("../devices", &read_state).await,
            Err(DeviceFailure::Protocol)
        );
    }

    #[tokio::test]
    async fn an_empty_or_admin_grant_is_refused_before_it_is_sent() {
        let (client, _) = client();
        assert_eq!(
            client.device_grant("dev-a1", &[]).await,
            Err(DeviceFailure::Protocol)
        );
        for admin in ["admin", "ADMIN", "Admin"] {
            let grant = ["read_state".to_string(), admin.to_string()];
            assert_eq!(
                client.device_grant("dev-a1", &grant).await,
                Err(DeviceFailure::Protocol)
            );
        }
    }

    #[test]
    fn a_grant_conflict_becomes_not_pending_and_the_rest_stays() {
        assert_eq!(
            not_pending(DeviceFailure::from_core(status(409))),
            DeviceFailure::NotPending
        );
        assert_eq!(
            not_pending(DeviceFailure::from_core(status(400))),
            DeviceFailure::Refused { status: 400 }
        );
        assert_eq!(
            serde_json::to_value(DeviceFailure::NotPending).unwrap(),
            json!({ "kind": "notPending" })
        );
    }

    #[test]
    fn each_refusal_becomes_the_kind_the_screen_words() {
        assert_eq!(
            DeviceFailure::from_core(CoreError::Unauthorized),
            DeviceFailure::Unauthorized
        );
        assert_eq!(
            DeviceFailure::from_core(CoreError::Forbidden),
            DeviceFailure::Forbidden
        );
        assert_eq!(
            DeviceFailure::from_core(CoreError::Transport("sin red".to_string())),
            DeviceFailure::Offline
        );
        assert_eq!(
            DeviceFailure::from_core(status(404)),
            DeviceFailure::NotFound
        );
        assert_eq!(
            DeviceFailure::from_core(status(409)),
            DeviceFailure::Refused { status: 409 }
        );
        assert_eq!(
            DeviceFailure::from_core(CoreError::PinMismatch),
            DeviceFailure::Protocol
        );
    }

    #[test]
    fn a_failure_reaches_the_screen_as_a_kind() {
        assert_eq!(
            serde_json::to_value(DeviceFailure::NotFound).unwrap(),
            json!({ "kind": "notFound" })
        );
        assert_eq!(
            serde_json::to_value(DeviceFailure::Refused { status: 409 }).unwrap(),
            json!({ "kind": "refused", "status": 409 })
        );
    }

    #[test]
    fn the_host_list_reads_the_contract() {
        let body = r#"{"ok":true,"devices":[
            {"device_id":"dev-a1","curve":"p256","label":"Pixel 9","owner_id":"12345",
             "created_at":1730000000.0,"last_seen_at":1730000100.5,"revoked":false,
             "permissions":["read_state"]},
            {"device_id":"dev-b2","label":"Viejo","created_at":1720000000.0,
             "last_seen_at":null,"revoked":true,"permissions":[]},
            {"device_id":"dev-c3","label":"Nuevo","created_at":1730000200.0,
             "last_seen_at":null,"revoked":false,"permissions":[],"sas":"097665"}]}"#;
        let response: DevicesResponse = serde_json::from_str(body).unwrap();
        assert_eq!(response.devices[0].label.as_deref(), Some("Pixel 9"));
        assert_eq!(response.devices[0].last_seen_at, Some(1730000100.5));
        assert_eq!(response.devices[0].sas, None);
        assert_eq!(response.devices[1].last_seen_at, None);
        assert!(response.devices[1].revoked);
        assert_eq!(response.devices[2].sas.as_deref(), Some("097665"));
    }

    #[test]
    fn a_device_enrolled_without_a_name_does_not_break_the_list() {
        let body = r#"{"ok":true,"devices":[
            {"device_id":"dev-d4","label":null,"owner_id":"1","created_at":1730000300.0,
             "last_seen_at":null,"revoked":false,"permissions":["read_state"]}]}"#;
        let response: DevicesResponse = serde_json::from_str(body).unwrap();
        assert_eq!(response.devices[0].label, None);
    }
}
