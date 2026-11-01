//! The agenda matrix (AGENDA-REST): notes and contacts. Reading them is `READ_STATE`.
//! Creating and editing one is `RESPOND`, which an agent also holds, so a device may
//! keep what the user asks it to. Deleting one, and importing the phone book, change
//! what the user keeps for good, so they need `APPROVE`. An `.ics` import only adds the
//! events a file holds and skips the ones it already has, so it is `RESPOND`. A route not
//! listed here fails closed.

use super::{rule, Budget, EndpointRule};
use crate::store::Permission;

pub(super) fn agenda_rule(method: &str, segments: &[&str]) -> Option<EndpointRule> {
    let rule = match (segments, method) {
        (["", "v1", "notes"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "notes", _id], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "notes"], "POST") => rule(Permission::Respond, Budget::Write),
        (["", "v1", "notes", _id, "update"], "POST") => rule(Permission::Respond, Budget::Write),
        (["", "v1", "notes", _id, "delete"], "POST") => rule(Permission::Approve, Budget::Write),
        (["", "v1", "contacts"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "contacts", id], "GET") if *id != "import" => {
            rule(Permission::ReadState, Budget::Read)
        }
        (["", "v1", "contacts"], "POST") => rule(Permission::Respond, Budget::Write),
        (["", "v1", "contacts", _id, "update"], "POST") => rule(Permission::Respond, Budget::Write),
        (["", "v1", "contacts", _id, "delete"], "POST") => rule(Permission::Approve, Budget::Write),
        (["", "v1", "contacts", "import"], "POST") => rule(Permission::Approve, Budget::Write),
        (["", "v1", "calendar", "events"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "calendar", "events", _id], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "calendar", "export.ics"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "calendar", "import"], "POST") => rule(Permission::Respond, Budget::Write),
        (["", "v1", "calendar", "events"], "POST") => rule(Permission::Respond, Budget::Write),
        (["", "v1", "calendar", "events", _id, "update"], "POST") => {
            rule(Permission::Respond, Budget::Write)
        }
        (["", "v1", "calendar", "events", _id, "delete"], "POST") => {
            rule(Permission::Approve, Budget::Write)
        }
        (["", "v1", "company", "timezone"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "company", "timezone"], "POST") => rule(Permission::Approve, Budget::Write),
        (["", "v1", "alerts", "rules"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "alerts", "mode"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "alerts", "rules"], "POST") => rule(Permission::Respond, Budget::Write),
        (["", "v1", "alerts", "rules", _id, "update"], "POST") => {
            rule(Permission::Respond, Budget::Write)
        }
        (["", "v1", "alerts", "rules", _id, "delete"], "POST") => {
            rule(Permission::Respond, Budget::Write)
        }
        (["", "v1", "alerts", "mode"], "POST") => rule(Permission::Respond, Budget::Write),
        _ => return None,
    };
    Some(rule)
}

#[cfg(test)]
mod tests {
    use crate::authz::{authorize, classify, Budget};
    use crate::store::Permission;

    /// Every write of the agenda, with the permission each one needs.
    const WRITES: [(&str, &str, Permission); 16] = [
        ("POST", "/v1/notes", Permission::Respond),
        ("POST", "/v1/notes/nt_1/update", Permission::Respond),
        ("POST", "/v1/notes/nt_1/delete", Permission::Approve),
        ("POST", "/v1/contacts", Permission::Respond),
        ("POST", "/v1/contacts/ct_1/update", Permission::Respond),
        ("POST", "/v1/contacts/ct_1/delete", Permission::Approve),
        ("POST", "/v1/contacts/import", Permission::Approve),
        ("POST", "/v1/calendar/events", Permission::Respond),
        ("POST", "/v1/calendar/import", Permission::Respond),
        (
            "POST",
            "/v1/calendar/events/ev_1/update",
            Permission::Respond,
        ),
        (
            "POST",
            "/v1/calendar/events/ev_1/delete",
            Permission::Approve,
        ),
        ("POST", "/v1/company/timezone", Permission::Approve),
        ("POST", "/v1/alerts/rules", Permission::Respond),
        ("POST", "/v1/alerts/rules/ar_1/update", Permission::Respond),
        ("POST", "/v1/alerts/rules/ar_1/delete", Permission::Respond),
        ("POST", "/v1/alerts/mode", Permission::Respond),
    ];

    #[test]
    fn every_write_of_the_agenda_needs_its_permission() {
        for (method, path, needed) in WRITES {
            let rule = classify(method, path).expect(path);
            assert_eq!(rule.permission, Some(needed), "{path}");
            assert_eq!(rule.budget, Budget::Write, "{path}");
        }
    }

    #[test]
    fn reading_the_agenda_needs_read_state() {
        for path in [
            "/v1/notes",
            "/v1/notes/nt_1",
            "/v1/contacts",
            "/v1/contacts/ct_1",
            "/v1/calendar/events",
            "/v1/calendar/events/ev_1",
            "/v1/calendar/export.ics",
            "/v1/company/timezone",
            "/v1/alerts/rules",
            "/v1/alerts/mode",
        ] {
            let rule = classify("GET", path).expect(path);
            assert_eq!(rule.permission, Some(Permission::ReadState), "{path}");
            assert_eq!(rule.budget, Budget::Read, "{path}");
        }
    }

    /// Holding `RESPOND` lets a device keep what it is asked to; a delete or a phone-book
    /// import needs `APPROVE` on top of it.
    #[test]
    fn respond_writes_but_never_deletes_or_imports_contacts() {
        let device = [Permission::ReadState, Permission::Respond];
        for (method, path, needed) in WRITES {
            let rule = classify(method, path).expect(path);
            let allowed = authorize(&device, &rule).is_ok();
            assert_eq!(allowed, needed == Permission::Respond, "{path}");
        }
    }

    #[test]
    fn unlisted_agenda_routes_fail_closed() {
        assert!(classify("DELETE", "/v1/notes/nt_1").is_err());
        assert!(classify("POST", "/v1/notes/nt_1").is_err());
        assert!(classify("GET", "/v1/notes/nt_1/update").is_err());
        assert!(classify("GET", "/v1/contacts/import").is_err());
        assert!(classify("POST", "/v1/contacts/ct_1/merge").is_err());
        assert!(classify("DELETE", "/v1/calendar/events/ev_1").is_err());
        assert!(classify("POST", "/v1/calendar/events/ev_1").is_err());
        assert!(classify("POST", "/v1/calendar/events/ev_1/move").is_err());
        assert!(classify("GET", "/v1/company/timezone/now").is_err());
        assert!(classify("POST", "/v1/calendar/export.ics").is_err());
        assert!(classify("GET", "/v1/calendar/import").is_err());
        assert!(classify("DELETE", "/v1/alerts/rules/ar_1").is_err());
        assert!(classify("GET", "/v1/alerts/rules/ar_1/update").is_err());
        assert!(classify("POST", "/v1/alerts/rules/ar_1").is_err());
        assert!(classify("POST", "/v1/alerts/mode/now").is_err());
    }
}
