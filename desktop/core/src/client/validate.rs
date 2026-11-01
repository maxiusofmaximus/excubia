use crate::error::{CoreError, Result};

pub fn validate_device_id(value: &str) -> Result<()> {
    let value = value.trim();
    if value.is_empty() || value.len() > 64 {
        return Err(CoreError::InvalidInput(
            "el identificador debe tener entre 1 y 64 caracteres".to_string(),
        ));
    }
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(CoreError::InvalidInput(
            "el identificador solo admite letras, números, '-' y '_'".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_pane_id(pane_id: &str) -> Result<()> {
    let Some((workspace, pane)) = pane_id.split_once(":p") else {
        return Err(CoreError::InvalidInput("panel inválido".to_string()));
    };
    let valid = |value: &str| {
        !value.is_empty()
            && value.len() <= 32
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
    };
    if !valid(workspace) || !valid(pane) {
        return Err(CoreError::InvalidInput("panel inválido".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_ids_follow_the_gateway_regex() {
        assert!(validate_pane_id("w1:p3").is_ok());
        assert!(validate_pane_id("wQ:p3N").is_ok());
        assert!(validate_pane_id("w1").is_err());
        assert!(validate_pane_id("w-1:p3").is_err());
        assert!(validate_pane_id("herdr agent").is_err());
    }

    #[test]
    fn device_ids_are_safe_for_a_header() {
        assert!(validate_device_id("dev-0123456789abcdef").is_ok());
        assert!(validate_device_id("hn-test_1").is_ok());
        assert!(validate_device_id("con espacio").is_err());
        assert!(validate_device_id("").is_err());
    }
}
