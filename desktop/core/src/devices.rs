//! «Mis dispositivos»: the devices the host trusts, read and revoked over
//! `/v1/devices`. Both need `ADMIN`, which only the operator's desktop holds. The
//! list carries public material only: never a key, a token or a push endpoint.

use reqwest::{Method, RequestBuilder};
use serde::de::{DeserializeOwned, IgnoredAny};
use serde::{Deserialize, Serialize};

use crate::client::{expect_json, validate_device_id, HostClient};
use crate::error::CoreError;

const DEVICES: &str = "/v1/devices";

/// One paired device, as the host lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Device {
    pub device_id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub created_at: f64,
    /// Seconds since the epoch; `None` for a device that never signed a request.
    #[serde(default)]
    pub last_seen_at: Option<f64>,
    #[serde(default)]
    pub revoked: bool,
    #[serde(default)]
    pub permissions: Vec<String>,
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

    async fn device_call<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
    ) -> DeviceResult<T> {
        let response = self
            .device_request(method, path)?
            .send()
            .await
            .map_err(|error| DeviceFailure::from_core(CoreError::from(error)))?;
        expect_json(response)
            .await
            .map_err(DeviceFailure::from_core)
    }

    /// Both calls here carry no body: the signature covers the empty digest.
    fn device_request(&self, method: Method, path: &str) -> DeviceResult<RequestBuilder> {
        self.signed_call(method, path, &[], None)
            .map_err(DeviceFailure::from_core)
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
            .device_request(Method::GET, DEVICES)
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
            .device_request(Method::POST, path)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request.method(), Method::POST);
        assert_eq!(request.url().path(), path);
        assert!(request.body().is_none());
        assert_signed(&request, &key, b"");
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
             "last_seen_at":null,"revoked":true,"permissions":[]}]}"#;
        let response: DevicesResponse = serde_json::from_str(body).unwrap();
        assert_eq!(response.devices[0].label, "Pixel 9");
        assert_eq!(response.devices[0].last_seen_at, Some(1730000100.5));
        assert_eq!(response.devices[1].last_seen_at, None);
        assert!(response.devices[1].revoked);
    }
}
