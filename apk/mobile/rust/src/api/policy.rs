//! Build-time policy for addresses that only make sense while developing.

/// Loopback is the `adb reverse` development path; a published build must not
/// accept it, because on a real phone it points at the phone itself.
#[flutter_rust_bridge::frb(sync)]
pub fn loopback_allowed() -> bool {
    loopback_allowed_in(cfg!(debug_assertions))
}

pub(crate) fn loopback_allowed_in(is_debug: bool) -> bool {
    is_debug
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_is_only_accepted_in_debug_builds() {
        assert!(loopback_allowed_in(true), "debug keeps adb reverse working");
        assert!(!loopback_allowed_in(false), "release must refuse loopback");
    }

    #[test]
    fn the_bridge_answer_tracks_the_crate_profile() {
        assert_eq!(loopback_allowed(), cfg!(debug_assertions));
    }
}
