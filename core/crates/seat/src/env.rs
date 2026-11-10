//! The seat's environment variables. Hooks and tools started before the rename
//! carry the `HN_*` names, so each variable is read under its `EXCUBIA_*` name
//! first and under the legacy name second. Nothing is ever written under the
//! legacy name.

/// The value of `current`, or of `legacy` when `current` is absent.
/// legacy: `HN_*` names of runs started before the rename; read only as a fallback.
pub fn var_or_legacy(current: &str, legacy: &str) -> Option<String> {
    lookup_or_legacy(current, legacy, |name| std::env::var(name).ok())
}

fn lookup_or_legacy(
    current: &str,
    legacy: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    lookup(current).or_else(|| lookup(legacy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_excubia_name_wins_and_the_legacy_name_is_the_fallback() {
        let env = |name: &str| match name {
            "EXCUBIA_TOOL_URL" => Some("new".to_string()),
            "HN_TOOL_URL" => Some("old".to_string()),
            "HN_ONLY" => Some("legacy-only".to_string()),
            _ => None,
        };
        assert_eq!(
            lookup_or_legacy("EXCUBIA_TOOL_URL", "HN_TOOL_URL", env).as_deref(),
            Some("new")
        );
        assert_eq!(
            lookup_or_legacy("EXCUBIA_ONLY", "HN_ONLY", env).as_deref(),
            Some("legacy-only")
        );
        assert_eq!(lookup_or_legacy("EXCUBIA_NONE", "HN_NONE", env), None);
    }
}
