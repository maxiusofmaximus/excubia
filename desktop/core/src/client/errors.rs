use reqwest::{Response, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{CoreError, Result};

#[derive(Debug, Default)]
pub(super) struct DetailFields {
    pub(super) code: Option<String>,
    pub(super) field: Option<String>,
    pub(super) message: Option<String>,
}

pub(crate) async fn expect_json<T: DeserializeOwned>(response: Response) -> Result<T> {
    let status = response.status();
    let bytes = response.bytes().await?;
    if status == StatusCode::UNAUTHORIZED {
        return Err(CoreError::Unauthorized);
    }
    if status == StatusCode::FORBIDDEN {
        return Err(CoreError::Forbidden);
    }
    if !status.is_success() {
        return Err(status_error(status, &bytes));
    }
    serde_json::from_slice(&bytes).map_err(CoreError::from)
}

pub(crate) fn status_error(status: StatusCode, body: &[u8]) -> CoreError {
    if status == StatusCode::UNAUTHORIZED {
        return CoreError::Unauthorized;
    }
    if status == StatusCode::FORBIDDEN {
        return CoreError::Forbidden;
    }
    let detail = detail_fields(body);
    CoreError::Status {
        status: status.as_u16(),
        message: detail
            .message
            .or_else(|| detail.code.clone())
            .or_else(|| status.canonical_reason().map(str::to_string))
            .unwrap_or_else(|| "sin detalle".to_string()),
        code: detail.code,
        field: detail.field,
    }
}

/// `detail` is a string, an action `{code, message}` object, or a `422`
/// `{code, errors: [{loc, msg}]}` body.
pub(super) fn detail_fields(body: &[u8]) -> DetailFields {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return DetailFields::default();
    };
    match value.get("detail") {
        Some(Value::String(text)) => DetailFields {
            message: Some(text.clone()),
            ..DetailFields::default()
        },
        Some(Value::Object(fields)) => {
            let code = fields
                .get("code")
                .and_then(Value::as_str)
                .map(str::to_string);
            let message = fields
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string);
            let first = fields
                .get("errors")
                .and_then(Value::as_array)
                .and_then(|errors| errors.first());
            // A dedicated rejection carries `field` next to `code`; a body
            // validation error carries `errors[0].loc[0]`.
            let field = fields
                .get("field")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    first
                        .and_then(|error| error.get("loc"))
                        .and_then(Value::as_array)
                        .and_then(|loc| loc.first())
                        .and_then(Value::as_str)
                        .map(str::to_string)
                });
            let message = message.or_else(|| {
                first
                    .and_then(|error| error.get("msg"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
            DetailFields {
                code,
                field,
                message,
            }
        }
        _ => DetailFields::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dedicated_422_field_is_read() {
        let fields =
            detail_fields(br#"{"detail":{"code":"unknown_agent","field":"agent","message":"no"}}"#);
        assert_eq!(fields.field.as_deref(), Some("agent"));
        assert_eq!(fields.message.as_deref(), Some("no"));

        let validation = detail_fields(
            br#"{"detail":{"code":"invalid_body","errors":[{"loc":["cwd"],"msg":"bad"}]}}"#,
        );
        assert_eq!(validation.field.as_deref(), Some("cwd"));
        assert_eq!(validation.message.as_deref(), Some("bad"));
    }
}
