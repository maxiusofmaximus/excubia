use excubia_auth::{Permission, VerifiedDevice};
use excubia_chat::{HookScope, Reader};

use crate::error::ApiError;
use crate::middleware::HookAuth;

/// The scope of the token, or the typed refusal of the host token: an
/// approval is always opened, read and burned by the hook of a run or panel.
pub(super) fn scope_of(auth: &HookAuth) -> Result<&HookScope, ApiError> {
    match auth {
        HookAuth::Scoped(scope) => Ok(scope),
        HookAuth::Host => Err(ApiError::forbidden_code("hook_token_not_scoped")),
    }
}

/// Only a device with `APPROVE` reads the details of a mail action: the grant
/// that decides. A device that only reads the state sees the approval without them.
pub(super) fn reader_of(device: &VerifiedDevice) -> Reader {
    if device.has(Permission::Approve) {
        Reader::Operator
    } else {
        Reader::Other
    }
}
