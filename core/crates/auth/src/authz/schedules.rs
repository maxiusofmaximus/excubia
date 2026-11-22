//! The scheduled work (F4-CORE). Reading the schedules and their runs is `READ_STATE`.
//! Creating, editing, pausing, resuming, deleting and running one now are the user's
//! decisions, so they need `APPROVE`, which no agent holds. A route not listed here fails closed.

use super::{rule, Budget, EndpointRule};
use crate::store::Permission;

pub(super) fn schedule_rule(method: &str, segments: &[&str]) -> Option<EndpointRule> {
    let rule = match (segments, method) {
        (["", "v1", "schedules"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "schedules", _id, "runs"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "schedules"], "POST") => rule(Permission::Approve, Budget::Write),
        (["", "v1", "schedules", _id], "PUT" | "DELETE") => {
            rule(Permission::Approve, Budget::Write)
        }
        (["", "v1", "schedules", _id, "pause" | "resume" | "run_now"], "POST") => {
            rule(Permission::Approve, Budget::Write)
        }
        _ => return None,
    };
    Some(rule)
}

#[cfg(test)]
mod tests {
    use crate::authz::{authorize, classify, Budget};
    use crate::store::Permission;

    /// Every write of the schedules: create, edit, delete, pause, resume and run now.
    const WRITES: [(&str, &str); 6] = [
        ("POST", "/v1/schedules"),
        ("PUT", "/v1/schedules/sch_1"),
        ("DELETE", "/v1/schedules/sch_1"),
        ("POST", "/v1/schedules/sch_1/pause"),
        ("POST", "/v1/schedules/sch_1/resume"),
        ("POST", "/v1/schedules/sch_1/run_now"),
    ];

    #[test]
    fn every_write_of_the_schedules_needs_approve() {
        for (method, path) in WRITES {
            let rule = classify(method, path).expect(path);
            assert_eq!(rule.permission, Some(Permission::Approve), "{path}");
            assert_eq!(rule.budget, Budget::Write, "{path}");
        }
    }

    #[test]
    fn reading_the_schedules_and_their_runs_needs_read_state() {
        for path in ["/v1/schedules", "/v1/schedules/sch_1/runs"] {
            let rule = classify("GET", path).expect(path);
            assert_eq!(rule.permission, Some(Permission::ReadState), "{path}");
            assert_eq!(rule.budget, Budget::Read, "{path}");
        }
    }

    /// An agent is a device without `APPROVE`: holding `RESPOND` changes nothing.
    #[test]
    fn an_agent_cannot_keep_edit_pause_or_run_a_schedule() {
        let agent = [Permission::ReadState, Permission::Respond];
        for (method, path) in WRITES {
            let rule = classify(method, path).expect(path);
            assert!(authorize(&agent, &rule).is_err(), "{path}");
        }
    }

    #[test]
    fn unlisted_schedule_routes_fail_closed() {
        assert!(classify("PATCH", "/v1/schedules/sch_1").is_err());
        assert!(classify("GET", "/v1/schedules/sch_1").is_err());
        assert!(classify("POST", "/v1/schedules/sch_1/fire").is_err());
        assert!(classify("DELETE", "/v1/schedules/sch_1/runs").is_err());
    }
}
