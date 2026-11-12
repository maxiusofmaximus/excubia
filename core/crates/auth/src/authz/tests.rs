use super::*;

#[test]
fn the_matrix_classifies_every_route() {
    assert_eq!(
        classify("GET", "/v1/state").unwrap().permission,
        Some(Permission::ReadState)
    );
    assert_eq!(
        classify("GET", "/v1/state?x=1").unwrap().budget,
        Budget::Read,
        "the query string is not part of the match"
    );
    assert_eq!(
        classify("POST", "/v1/agents/w1:p1/respond")
            .unwrap()
            .permission,
        Some(Permission::Respond)
    );
    assert_eq!(
        classify("POST", "/v1/agents/w1:p1/keys")
            .unwrap()
            .permission,
        Some(Permission::SendKey)
    );
    assert_eq!(
        classify("POST", "/v1/agents/w1:p1/seen").unwrap().budget,
        Budget::None
    );
    assert_eq!(
        classify("POST", "/v1/voice/transcribe").unwrap().permission,
        Some(Permission::Respond)
    );
    assert_eq!(
        classify("GET", "/v1/devices").unwrap().permission,
        Some(Permission::Admin)
    );
    assert_eq!(
        classify("POST", "/v1/devices/dev-1/revoke").unwrap().budget,
        Budget::Write
    );
    assert_eq!(
        classify("POST", "/v1/devices/dev-1/grant")
            .unwrap()
            .permission,
        Some(Permission::Admin)
    );
    assert_eq!(
        classify("POST", "/v1/me/push-endpoint").unwrap().permission,
        Some(Permission::ReadState)
    );
    assert_eq!(
        classify("POST", "/v1/workspaces").unwrap().permission,
        Some(Permission::Respond)
    );
    assert_eq!(
        classify("GET", "/v1/catalog").unwrap().permission,
        Some(Permission::ReadState)
    );
    assert_eq!(
        classify("GET", "/v1/doc-tools").unwrap().permission,
        Some(Permission::ReadState)
    );
    assert_eq!(
        classify("GET", "/v1/doc-tools").unwrap().budget,
        Budget::Read
    );
    // Repointing an agent is the owner's decision: APPROVE, never an agent's RESPOND.
    assert_eq!(
        classify("POST", "/v1/catalog/codex/fix")
            .unwrap()
            .permission,
        Some(Permission::Approve)
    );
    assert!(
        classify("POST", "/v1/catalog/codex").is_err(),
        "no bare catalogue write"
    );
    assert_eq!(
        classify("GET", "/v1/roles").unwrap().permission,
        Some(Permission::ReadState)
    );
    assert_eq!(classify("GET", "/v1/roles").unwrap().budget, Budget::Read);
    assert!(
        classify("POST", "/v1/roles").is_err(),
        "read-only, fails closed"
    );
    assert_eq!(
        classify("POST", "/v1/agents/w1:p1/stop")
            .unwrap()
            .permission,
        Some(Permission::Respond)
    );
    assert!(classify("DELETE", "/v1/state").is_err(), "fails closed");
    assert!(classify("POST", "/v1/nope").is_err(), "no rule, no entry");
}

#[test]
fn the_chat_matrix_separates_reads_from_writes() {
    let reads = [
        ("GET", "/v1/chat/bootstrap"),
        ("GET", "/v1/chat/conversations"),
        ("GET", "/v1/chat/conversations/counts"),
        ("GET", "/v1/chat/conversations/cnv_1"),
        ("GET", "/v1/chat/conversations/cnv_1/timeline"),
        ("GET", "/v1/chat/conversations/cnv_1/messages"),
        ("GET", "/v1/chat/sections"),
        (
            "GET",
            "/v1/chat/conversations/cnv_1/attachments/att_1/preview",
        ),
    ];
    for (method, path) in reads {
        let rule = classify(method, path).expect(path);
        assert_eq!(rule.permission, Some(Permission::ReadState), "{path}");
        assert_eq!(rule.budget, Budget::Read, "{path}");
    }
    let writes = [
        ("POST", "/v1/chat/conversations"),
        ("POST", "/v1/chat/conversations/cnv_1/update"),
        ("POST", "/v1/chat/conversations/cnv_1/messages"),
        ("POST", "/v1/chat/conversations/cnv_1/stop"),
        ("POST", "/v1/chat/sections"),
    ];
    for (method, path) in writes {
        let rule = classify(method, path).expect(path);
        assert_eq!(rule.permission, Some(Permission::Respond), "{path}");
        assert_eq!(rule.budget, Budget::Write, "{path}");
    }
    let read = classify("POST", "/v1/chat/conversations/cnv_1/read").expect("read");
    assert_eq!(read.permission, Some(Permission::ReadState));
    assert_eq!(read.budget, Budget::None);
    let delete = classify("DELETE", "/v1/chat/conversations/cnv_1").expect("delete");
    assert_eq!(delete.permission, Some(Permission::Respond));
    assert_eq!(delete.budget, Budget::Write);
    assert!(classify("POST", "/v1/chat/bootstrap").is_err());
}

/// The approval surface: the inbox is a read and the decision spends the
/// `APPROVE` grant, which nothing else does.
#[test]
fn the_approval_matrix_separates_the_inbox_from_the_decision() {
    let inbox = classify("GET", "/v1/chat/approvals").expect("inbox");
    assert_eq!(inbox.permission, Some(Permission::ReadState));
    assert_eq!(inbox.budget, Budget::Read);
    let decision = classify("POST", "/v1/chat/approvals/apr_1/decision").expect("decision");
    assert_eq!(decision.permission, Some(Permission::Approve));
    assert_eq!(decision.budget, Budget::Write);
    // The hook routes are not device routes: they carry no rule and fail
    // closed for a signed client.
    assert!(classify("POST", "/v1/chat/approvals").is_err());
    assert!(classify("GET", "/v1/chat/approvals/apr_1").is_err());
    assert!(classify("POST", "/v1/chat/approvals/apr_1/consume").is_err());
}

#[test]
fn changing_the_approval_mode_needs_approve_not_admin() {
    let read = classify("GET", "/v1/chat/approvals/settings").expect("settings");
    assert_eq!(read.permission, Some(Permission::ReadState));
    let change = classify("POST", "/v1/chat/approvals/settings").expect("change");
    assert_eq!(change.permission, Some(Permission::Approve));
    assert_eq!(change.budget, Budget::Write);
    let admin_only = [Permission::Admin];
    assert!(authorize(&admin_only, &change).is_err());
}

#[test]
fn the_default_agent_is_the_owners_approval_and_its_listing_is_read_only() {
    let list = classify("GET", "/v1/catalog").expect("list");
    assert_eq!(list.permission, Some(Permission::ReadState));
    let set = classify("POST", "/v1/catalog/default").expect("set default");
    assert_eq!(set.permission, Some(Permission::Approve));
    assert_eq!(set.budget, Budget::Write);
    let agent = [Permission::Respond];
    assert!(
        authorize(&agent, &set).is_err(),
        "an agent cannot choose the default"
    );
}

/// Sending is the human's `APPROVE` grant: a `RESPOND` device, an agent or a
/// read-only device cannot send, cancel or read a send.
#[test]
fn sending_needs_approve_and_consent_needs_admin() {
    let send = classify("POST", "/v1/mail/drafts/AAMk-1/send").expect("send");
    assert_eq!(send.permission, Some(Permission::Approve));
    assert_eq!(send.budget, Budget::Write);
    let edit = classify("POST", "/v1/mail/drafts/AAMk-1/update").expect("edit");
    assert_eq!(edit.permission, Some(Permission::Approve));
    assert_eq!(edit.budget, Budget::Write);
    let review = classify("GET", "/v1/mail/drafts/AAMk-1?account=trabajo").expect("review");
    assert_eq!(review.permission, Some(Permission::Approve));
    assert_eq!(review.budget, Budget::Read);
    let cancel = classify("POST", "/v1/mail/sends/snd_1/cancel").expect("cancel");
    assert_eq!(cancel.permission, Some(Permission::Approve));
    let status = classify("GET", "/v1/mail/sends/snd_1").expect("status");
    assert_eq!(status.permission, Some(Permission::Approve));
    assert_eq!(status.budget, Budget::Read);
    let enable = classify("POST", "/v1/mail/send/enable").expect("enable");
    assert_eq!(enable.permission, Some(Permission::Admin));
    let disable = classify("POST", "/v1/mail/send/disable").expect("disable");
    assert_eq!(disable.permission, Some(Permission::Admin));
    let enable_status = classify("GET", "/v1/mail/send/enable/status").expect("status");
    assert_eq!(enable_status.permission, Some(Permission::Admin));

    let respond_only = [
        Permission::ReadState,
        Permission::Respond,
        Permission::Admin,
    ];
    assert!(authorize(&respond_only, &send).is_err());
    assert!(authorize(&respond_only, &cancel).is_err());
    assert!(authorize(&[Permission::Approve], &send).is_ok());
    assert!(classify("POST", "/v1/mail/drafts/AAMk-1").is_err());
    assert!(classify("DELETE", "/v1/mail/sends/snd_1").is_err());
}

#[test]
fn a_grant_is_required_for_protected_routes() {
    let respond = classify("POST", "/v1/agents/w1:p1/respond").unwrap();
    assert!(authorize(&[Permission::ReadState], &respond).is_err());
    assert!(authorize(&[Permission::ReadState, Permission::Respond], &respond).is_ok());
    let health = classify("GET", "/v1/health").unwrap();
    assert!(authorize(&[], &health).is_ok());
}

#[test]
fn the_pake_routes_are_public_posts_and_nothing_else_under_them_is() {
    for path in ["/v1/pair/pake/start", "/v1/pair/pake/finish"] {
        let rule = classify("POST", path).expect("listed");
        assert_eq!(
            (rule.permission, rule.budget),
            (None, Budget::None),
            "{path}"
        );
        assert!(classify("GET", path).is_err(), "{path} is POST only");
    }
    assert!(classify("POST", "/v1/pair/pake/other").is_err());
    assert!(
        classify("POST", "/v1/admin/pake-code").is_err(),
        "the admin route is not a device route"
    );
}
