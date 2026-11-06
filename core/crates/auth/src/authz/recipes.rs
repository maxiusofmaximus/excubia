//! The recipes (F5). Reading them is `READ_STATE`. Keeping, editing, deleting,
//! drafting and running one are the user's decisions, so they need `APPROVE`,
//! which no agent holds. A route not listed here fails closed.

use super::{rule, Budget, EndpointRule};
use crate::store::Permission;

pub(super) fn recipe_rule(method: &str, segments: &[&str]) -> Option<EndpointRule> {
    let rule = match (segments, method) {
        (["", "v1", "recipes"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "recipes"], "POST") => rule(Permission::Approve, Budget::Write),
        (["", "v1", "recipes", "draft"], "POST") => rule(Permission::Approve, Budget::Write),
        (["", "v1", "recipes", _id], "PUT" | "DELETE") => rule(Permission::Approve, Budget::Write),
        (["", "v1", "recipes", _id, "run"], "POST") => rule(Permission::Approve, Budget::Write),
        _ => return None,
    };
    Some(rule)
}

#[cfg(test)]
mod tests {
    use crate::authz::{authorize, classify, Budget};
    use crate::store::Permission;

    /// Every write of the recipes, the draft and the run.
    const WRITES: [(&str, &str); 5] = [
        ("POST", "/v1/recipes"),
        ("PUT", "/v1/recipes/rcp_1"),
        ("DELETE", "/v1/recipes/rcp_1"),
        ("POST", "/v1/recipes/draft"),
        ("POST", "/v1/recipes/rcp_1/run"),
    ];

    #[test]
    fn every_write_of_the_recipes_needs_approve() {
        for (method, path) in WRITES {
            let rule = classify(method, path).expect(path);
            assert_eq!(rule.permission, Some(Permission::Approve), "{path}");
            assert_eq!(rule.budget, Budget::Write, "{path}");
        }
    }

    #[test]
    fn reading_the_recipes_needs_read_state() {
        let rule = classify("GET", "/v1/recipes").expect("list");
        assert_eq!(rule.permission, Some(Permission::ReadState));
        assert_eq!(rule.budget, Budget::Read);
    }

    /// An agent is a device without `APPROVE`: holding `RESPOND` changes nothing.
    #[test]
    fn an_agent_cannot_keep_edit_delete_draft_or_run_a_recipe() {
        let agent = [Permission::ReadState, Permission::Respond];
        for (method, path) in WRITES {
            let rule = classify(method, path).expect(path);
            assert!(authorize(&agent, &rule).is_err(), "{path}");
        }
    }

    #[test]
    fn unlisted_recipe_routes_fail_closed() {
        assert!(classify("PATCH", "/v1/recipes/rcp_1").is_err());
        assert!(classify("GET", "/v1/recipes/rcp_1/run").is_err());
        assert!(classify("POST", "/v1/recipes/rcp_1").is_err());
        assert!(classify("DELETE", "/v1/recipes/rcp_1/run").is_err());
    }
}
