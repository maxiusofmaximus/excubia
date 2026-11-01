use crate::error::ApiError;

pub(super) const MAX_SUMMARY_CHARS: usize = 400;
pub(super) const MAX_TOOL_CHARS: usize = 100;
pub(super) const MAX_PATH_CHARS: usize = 400;
pub(super) const MAX_PATHS: usize = 32;
pub(super) const MAX_LABEL_CHARS: usize = 200;
pub(super) const MAX_EXCLUDE: usize = 20;
pub(super) const MAX_EXCLUDE_CHARS: usize = 512;

/// One bounded text field: no controls, a length ceiling and, when it is
/// required, not blank.
pub(super) fn bounded(
    raw: Option<&str>,
    field: &'static str,
    max_chars: usize,
    required: bool,
) -> Result<Option<String>, ApiError> {
    let Some(raw) = raw else {
        if required {
            return Err(ApiError::invalid_field(
                "invalid_body",
                field,
                format!("{field} is required"),
            ));
        }
        return Ok(None);
    };
    let value = raw.trim();
    if value.is_empty() {
        if required {
            return Err(ApiError::invalid_field(
                "invalid_body",
                field,
                format!("{field} cannot be blank"),
            ));
        }
        return Ok(None);
    }
    if value.chars().count() > max_chars {
        return Err(ApiError::invalid_field(
            "invalid_body",
            field,
            format!("{field} cannot exceed {max_chars} characters"),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(ApiError::invalid_field(
            "invalid_body",
            field,
            format!("{field} cannot contain control characters"),
        ));
    }
    Ok(Some(value.to_string()))
}

/// A bounded list of paths.
pub(super) fn bounded_list(
    raw: Vec<String>,
    field: &'static str,
    max_items: usize,
    max_chars: usize,
) -> Result<Vec<String>, ApiError> {
    if raw.len() > max_items {
        return Err(ApiError::invalid_field(
            "invalid_body",
            field,
            format!("{field} cannot hold more than {max_items} entries"),
        ));
    }
    let mut paths = Vec::with_capacity(raw.len());
    for value in raw {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if value.chars().count() > max_chars {
            return Err(ApiError::invalid_field(
                "invalid_body",
                field,
                format!("{field} cannot exceed {max_chars} characters"),
            ));
        }
        if value.chars().any(char::is_control) {
            return Err(ApiError::invalid_field(
                "invalid_body",
                field,
                format!("{field} cannot contain control characters"),
            ));
        }
        paths.push(value.to_string());
    }
    Ok(paths)
}
