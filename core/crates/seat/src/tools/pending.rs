//! `approval.pending`: the approvals waiting at the caller's level. A lead sees
//! the ones of its project, Alfred the ones of the company. It takes no
//! arguments, so a caller cannot widen its own scope, and it decides nothing.

use hn_core_chat::{ToolError, ToolRequest};
use serde::Deserialize;

use super::{invalid_arguments, ToolCall};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingArgs {}

pub fn parse(request: &ToolRequest) -> Result<ToolCall, ToolError> {
    let _: PendingArgs = serde_json::from_value(request.arguments.clone())
        .map_err(|error| invalid_arguments("arguments", error))?;
    Ok(ToolCall::ApprovalPending)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn request(arguments: Value) -> ToolRequest {
        ToolRequest {
            tool: "approval.pending".to_string(),
            arguments,
        }
    }

    #[test]
    fn the_listing_takes_no_arguments() {
        assert_eq!(parse(&request(json!({}))), Ok(ToolCall::ApprovalPending));

        // A filter that could name another project is refused, not applied.
        let widened = parse(&request(json!({ "project_id": "prj_other" })));
        assert_eq!(
            widened.expect_err("unknown field").code,
            "invalid_arguments"
        );
    }
}
