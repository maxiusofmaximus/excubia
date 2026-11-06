//! The restore-point matrix (F2-CORE). Listing points and what they would change is
//! `READ_STATE`. Putting a folder back is `APPROVE`, which no agent holds: it changes
//! the user's files, so it takes the same grant as the approvals.

use super::{rule, Budget, EndpointRule};
use crate::store::Permission;

pub(super) fn restore_rule(method: &str, segments: &[&str]) -> Option<EndpointRule> {
    let rule = match (segments, method) {
        (["", "v1", "restore", "points"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "restore", "points", _id, "files"], "GET") => {
            rule(Permission::ReadState, Budget::Read)
        }
        (["", "v1", "restore", "points", _id, "restore"], "POST") => {
            rule(Permission::Approve, Budget::Write)
        }
        _ => return None,
    };
    Some(rule)
}

#[cfg(test)]
mod tests {
    use crate::authz::{classify, Budget};
    use crate::store::Permission;

    #[test]
    fn reading_points_needs_read_state() {
        for path in [
            "/v1/restore/points?agent=w1:p1",
            "/v1/restore/points/01ARZ3NDEKTSV4RRFFQ69G5FAV/files",
        ] {
            let rule = classify("GET", path).expect(path);
            assert_eq!(rule.permission, Some(Permission::ReadState), "{path}");
            assert_eq!(rule.budget, Budget::Read, "{path}");
        }
    }

    #[test]
    fn putting_a_point_back_needs_approve() {
        let path = "/v1/restore/points/01ARZ3NDEKTSV4RRFFQ69G5FAV/restore";
        let rule = classify("POST", path).expect(path);
        assert_eq!(rule.permission, Some(Permission::Approve));
        assert_eq!(rule.budget, Budget::Write);
    }

    #[test]
    fn an_unlisted_restore_route_fails_closed() {
        assert!(classify("DELETE", "/v1/restore/points/01ARZ3NDEKTSV4RRFFQ69G5FAV").is_err());
    }
}
