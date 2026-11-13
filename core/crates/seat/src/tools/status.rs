//! `status.overview`: Alfred's read of the company. It takes no arguments, so a
//! caller cannot widen what it sees, and it changes nothing.

use excubia_chat::{ToolError, ToolRequest};

use super::{no_arguments, ToolCall};

pub fn parse(request: &ToolRequest) -> Result<ToolCall, ToolError> {
    no_arguments(request)?;
    Ok(ToolCall::StatusOverview)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn request(arguments: Value) -> ToolRequest {
        ToolRequest {
            tool: "status.overview".to_string(),
            arguments,
        }
    }

    #[test]
    fn the_overview_takes_no_arguments() {
        assert_eq!(parse(&request(json!({}))), Ok(ToolCall::StatusOverview));

        // A path or a filter is refused, never applied to the view.
        let widened = parse(&request(json!({ "root_path": "C:\\" })));
        assert_eq!(
            widened.expect_err("unknown field").code,
            "invalid_arguments"
        );
    }
}
