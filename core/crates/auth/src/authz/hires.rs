//! The team matrix (ONBOARD-T3). Contracting, renaming, changing the avatar and
//! dismissing are the user's decisions, so they need `APPROVE`, which no agent
//! holds. Reading the team is `READ_STATE`. A route not listed here fails closed.

use super::{rule, Budget, EndpointRule};
use crate::store::Permission;

pub(super) fn hire_rule(method: &str, segments: &[&str]) -> Option<EndpointRule> {
    let rule = match (segments, method) {
        (["", "v1", "hires"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "hires"], "POST") => rule(Permission::Approve, Budget::Write),
        (["", "v1", "hires", _id, "update" | "dismiss"], "POST") => {
            rule(Permission::Approve, Budget::Write)
        }
        (["", "v1", "onboarding"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "onboarding"], "POST") => rule(Permission::Approve, Budget::Write),
        _ => return None,
    };
    Some(rule)
}

#[cfg(test)]
mod tests {
    use crate::authz::{authorize, classify, Budget};
    use crate::store::Permission;

    /// Every write of the team surface.
    const WRITES: [(&str, &str); 4] = [
        ("POST", "/v1/hires"),
        ("POST", "/v1/hires/hire_1/update"),
        ("POST", "/v1/hires/hire_1/dismiss"),
        ("POST", "/v1/onboarding"),
    ];

    #[test]
    fn every_write_of_the_team_needs_approve() {
        for (method, path) in WRITES {
            let rule = classify(method, path).expect(path);
            assert_eq!(rule.permission, Some(Permission::Approve), "{path}");
            assert_eq!(rule.budget, Budget::Write, "{path}");
        }
    }

    #[test]
    fn reading_the_team_needs_read_state() {
        for path in ["/v1/hires", "/v1/onboarding"] {
            let rule = classify("GET", path).expect(path);
            assert_eq!(rule.permission, Some(Permission::ReadState), "{path}");
            assert_eq!(rule.budget, Budget::Read, "{path}");
        }
    }

    /// An agent is a device without `APPROVE`: holding `RESPOND` changes nothing.
    #[test]
    fn an_agent_cannot_write_the_team() {
        let agent = [Permission::ReadState, Permission::Respond];
        for (method, path) in WRITES {
            let rule = classify(method, path).expect(path);
            assert!(authorize(&agent, &rule).is_err(), "{path}");
        }
    }

    #[test]
    fn unlisted_team_routes_fail_closed() {
        assert!(classify("DELETE", "/v1/hires/hire_1").is_err());
        assert!(classify("POST", "/v1/hires/hire_1").is_err());
        assert!(classify("GET", "/v1/hires/hire_1/update").is_err());
        assert!(classify("POST", "/v1/onboarding/x").is_err());
    }
}
