//! The plan matrix (F3). Reading the plans is `READ_STATE`; approving, rejecting
//! or asking for changes is `APPROVE`, the same grant that decides an approval.
//! An agent holds `RESPOND`, never `APPROVE`, so it cannot answer its own plan.

use super::{rule, Budget, EndpointRule};
use crate::store::Permission;

pub(super) fn plan_rule(method: &str, segments: &[&str]) -> Option<EndpointRule> {
    let rule = match (segments, method) {
        (["", "v1", "plans"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "plans", _id, "approve" | "reject" | "request_changes"], "POST") => {
            rule(Permission::Approve, Budget::Write)
        }
        _ => return None,
    };
    Some(rule)
}

#[cfg(test)]
mod tests {
    use super::super::{authorize, classify};
    use super::*;

    const WRITES: [&str; 3] = [
        "/v1/plans/plan_1/approve",
        "/v1/plans/plan_1/reject",
        "/v1/plans/plan_1/request_changes",
    ];

    #[test]
    fn answering_a_plan_needs_approve_and_reading_needs_read_state() {
        for path in WRITES {
            let rule = classify("POST", path).expect(path);
            assert_eq!(rule.permission, Some(Permission::Approve), "{path}");
            assert_eq!(rule.budget, Budget::Write, "{path}");
        }
        let rule = classify("GET", "/v1/plans?status=proposed").expect("list");
        assert_eq!(rule.permission, Some(Permission::ReadState));
        assert_eq!(rule.budget, Budget::Read);
    }

    #[test]
    fn an_agent_without_approve_cannot_answer_a_plan() {
        let agent = [Permission::ReadState, Permission::Respond];
        for path in WRITES {
            let rule = classify("POST", path).expect(path);
            assert!(authorize(&agent, &rule).is_err(), "{path}");
        }
        let unknown = classify("POST", "/v1/plans/plan_1/delete");
        assert!(unknown.is_err(), "unlisted plan routes fail closed");
    }
}
