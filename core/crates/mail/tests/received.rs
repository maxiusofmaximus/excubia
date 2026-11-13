//! The checks a received mail gets from the host (MAIL-LECTURA). They are pure
//! functions of the headers and the body, so each verdict is one row of a table.

use excubia_mail::received::{
    auth_summary, check_message, claims_attachment, clean_body, cut, kept_headers, links, one_line,
    AttachmentInfo, AuthSummary, MAX_BODY_CHARS,
};

fn auth(spf: &'static str, dkim: &'static str, dmarc: &'static str) -> AuthSummary {
    AuthSummary { spf, dkim, dmarc }
}

fn file(name: &str, is_inline: bool) -> AttachmentInfo {
    AttachmentInfo {
        name: name.to_owned(),
        content_type: "application/pdf".to_owned(),
        size: 10,
        is_inline,
    }
}

/// The headers of a mail whose only header that matters is its Return-Path.
fn returned(path: &str) -> Vec<(String, String)> {
    vec![("Return-Path".to_owned(), path.to_owned())]
}

/// A table row: a name, the headers of a mail, and its (list_unsubscribe, one_click, bulk).
type Row = (
    &'static str,
    &'static [(&'static str, &'static str)],
    (bool, bool, bool),
);

/// The bulk flags of a mail with the given headers: (list_unsubscribe, one_click, bulk).
fn bulk_flags(headers: &[(&str, &str)]) -> (bool, bool, bool) {
    let pairs: Vec<(String, String)> = headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    let checks = check_message(
        "a@x.es",
        &[],
        &pairs,
        auth("pass", "pass", "pass"),
        "",
        &[],
        &[],
    );
    (
        checks.list_unsubscribe,
        checks.one_click_unsubscribe,
        checks.bulk,
    )
}

#[test]
fn the_list_headers_are_read_by_name_value_and_case() {
    let cases: [Row; 9] = [
        ("no headers", &[], (false, false, false)),
        (
            "list-unsubscribe alone",
            &[("List-Unsubscribe", "<mailto:b@x.es>")],
            (true, false, true),
        ),
        (
            "one-click in any case",
            &[
                ("list-unsubscribe", "<https://x.es/u>"),
                ("LIST-UNSUBSCRIBE-POST", "List-Unsubscribe=One-Click"),
            ],
            (true, true, true),
        ),
        (
            "post without one-click",
            &[("List-Unsubscribe-Post", "Please-Do-It")],
            (false, false, false),
        ),
        (
            "precedence bulk",
            &[("Precedence", "bulk")],
            (false, false, true),
        ),
        (
            "precedence list, spaced and in capitals",
            &[("Precedence", " LIST ")],
            (false, false, true),
        ),
        (
            "precedence junk",
            &[("Precedence", "junk")],
            (false, false, true),
        ),
        (
            "precedence first-class is not bulk",
            &[("Precedence", "first-class")],
            (false, false, false),
        ),
        (
            "empty list-unsubscribe still exists",
            &[("List-Unsubscribe", "")],
            (true, false, true),
        ),
    ];
    for (name, headers, expected) in cases {
        assert_eq!(bulk_flags(headers), expected, "{name}");
    }
}

#[test]
fn auth_reads_each_method_from_authentication_results() {
    let header = "mx.example.com; spf=pass smtp.mailfrom=a@b.c; dkim=pass header.d=b.c; dmarc=pass action=none";
    assert_eq!(auth_summary(&[header], None), auth("pass", "pass", "pass"));
}

#[test]
fn auth_is_missing_when_the_mail_names_no_result() {
    assert_eq!(
        auth_summary(&[], None),
        auth("missing", "missing", "missing")
    );
}

#[test]
fn received_spf_backs_up_spf_only() {
    assert_eq!(
        auth_summary(&[], Some("softfail (google.com: domain of x.com ...)")),
        auth("softfail", "missing", "missing")
    );
    // Authentication-Results decides SPF whenever it names one.
    assert_eq!(
        auth_summary(&["mx; spf=pass"], Some("fail")),
        auth("pass", "missing", "missing")
    );
}

#[test]
fn values_match_in_any_case_and_unknown_words_count_as_missing() {
    assert_eq!(
        auth_summary(&["MX; SPF=PASS; DKIM=Fail; DMARC=None"], None),
        auth("pass", "fail", "none")
    );
    assert_eq!(
        auth_summary(&["mx; spf=bogus; dkim=pass"], None),
        auth("missing", "pass", "missing")
    );
}

#[test]
fn a_tag_inside_another_name_does_not_count() {
    assert_eq!(
        auth_summary(&["mx; x-dkim=pass; dkim=fail"], None),
        auth("missing", "fail", "missing")
    );
}

#[test]
fn the_topmost_header_that_names_a_method_decides_it() {
    assert_eq!(
        auth_summary(&["a.example; dkim=fail", "b.example; dkim=pass"], None).dkim,
        "fail"
    );
}

#[test]
fn only_the_headers_the_reader_needs_leave_the_host() {
    let headers = kept_headers(&[
        ("Return-Path".into(), "<bounce@spam.example>".into()),
        ("reply-to".into(), "ana@otro.net".into()),
        ("Message-ID".into(), "x".repeat(1_500)),
        ("X-Secret".into(), "token".into()),
    ]);
    assert_eq!(
        headers.return_path.as_deref(),
        Some("<bounce@spam.example>")
    );
    assert_eq!(headers.reply_to.as_deref(), Some("ana@otro.net"));
    assert_eq!(
        headers.message_id.as_ref().map(|id| id.chars().count()),
        Some(1_000)
    );

    let shown = serde_json::to_value(&headers).expect("json");
    assert!(shown.get("authentication_results").is_none());
    assert!(!shown.to_string().contains("token"));
}

#[test]
fn links_keep_only_web_addresses_with_their_visible_text() {
    let html = r#"<p><a href="https://Bank.Example.com/login">banco.es</a>
        <a href="mailto:ana@x.com">escribir</a>
        <a href='http://evil.test/p?q=1'>aquí</a>
        <a href="ftp://files.example">archivos</a></p>"#;
    let found = links(html);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].host, "bank.example.com");
    assert_eq!(found[0].text, "banco.es");
    assert_eq!(found[1].host, "evil.test");
    assert_eq!(found[1].text, "aquí");
}

#[test]
fn matching_addresses_pass_every_check() {
    let checks = check_message(
        "ana@Ejemplo.es",
        &["ana@ejemplo.es".to_owned()],
        &returned("<ana@ejemplo.es>"),
        auth("pass", "pass", "pass"),
        "Te envío el informe de ayer.",
        &[],
        &[],
    );
    assert_eq!(checks.from_domain.as_deref(), Some("ejemplo.es"));
    assert_eq!(checks.reply_to_domain.as_deref(), Some("ejemplo.es"));
    assert_eq!(checks.return_path_domain.as_deref(), Some("ejemplo.es"));
    assert!(!checks.reply_to_differs);
    assert!(!checks.return_path_differs);
    assert!(!checks.claims_attachment_but_none);
    assert!(!checks.link_host_differs);
}

#[test]
fn a_reply_to_or_return_path_of_another_domain_is_flagged() {
    let checks = check_message(
        "avisos@banco.es",
        &["phish@otro.net".to_owned()],
        &returned("<bounce@spam.example>"),
        auth("fail", "none", "fail"),
        "",
        &[],
        &[],
    );
    assert_eq!(checks.reply_to_domain.as_deref(), Some("otro.net"));
    assert!(checks.reply_to_differs);
    assert_eq!(checks.return_path_domain.as_deref(), Some("spam.example"));
    assert!(checks.return_path_differs);

    // Any reply-to of another domain counts, not only the first.
    let second = check_message(
        "avisos@banco.es",
        &["ana@banco.es".to_owned(), "x@otro.net".to_owned()],
        &[],
        auth("pass", "pass", "pass"),
        "",
        &[],
        &[],
    );
    assert_eq!(second.reply_to_domain.as_deref(), Some("banco.es"));
    assert!(second.reply_to_differs);
}

#[test]
fn an_announced_attachment_that_is_not_there_is_flagged() {
    let claim = |text: &str, attachments: &[AttachmentInfo]| {
        check_message(
            "a@x.es",
            &[],
            &[],
            auth("pass", "pass", "pass"),
            text,
            attachments,
            &[],
        )
        .claims_attachment_but_none
    };
    assert!(claim("Adjunto la factura.", &[]));
    assert!(!claim("Adjunto la factura.", &[file("factura.pdf", false)]));
    // An inline image is not an attachment the mail can be held to.
    assert!(claim("Attached the file", &[file("logo.png", true)]));
    assert!(!claim("Gracias por todo.", &[]));
}

#[test]
fn the_attachment_words_match_in_any_case() {
    assert!(claims_attachment("Te mando el ADJUNTO"));
    assert!(claims_attachment("see attachment"));
    assert!(!claims_attachment("Nos vemos mañana."));
}

#[test]
fn a_link_that_shows_another_host_is_flagged() {
    let ok = auth("pass", "pass", "pass");
    let phish = links(r#"<a href="https://phish.example/x">banco.es</a>"#);
    assert!(
        check_message("avisos@banco.es", &[], &[], ok.clone(), "", &[], &phish).link_host_differs
    );

    let same = links(r#"<a href="https://www.banco.es/x">banco.es</a>"#);
    assert!(!check_message("avisos@banco.es", &[], &[], ok, "", &[], &same).link_host_differs);
}

#[test]
fn the_body_keeps_its_line_breaks_and_loses_other_controls() {
    assert_eq!(
        clean_body("uno\u{7}\ndos\tcuatro\r", "text"),
        "uno\ndos\tcuatro\r"
    );
    let html = clean_body("<p>Hola<br>mundo</p>", "html");
    assert!(html.contains("Hola") && html.contains("mundo"));
}

#[test]
fn a_long_text_is_cut_by_characters_and_says_so() {
    assert_eq!(cut("ñandú", 3), ("ñan".to_owned(), true));
    assert_eq!(cut("abc", 3), ("abc".to_owned(), false));
    assert!(cut(&"a".repeat(MAX_BODY_CHARS + 1), MAX_BODY_CHARS).1);
}

#[test]
fn a_line_has_no_controls_and_stays_within_its_bound() {
    assert_eq!(one_line("a\nb\u{7}c", 10), "a b c");
    assert_eq!(one_line("abcdef", 3), "abc");
}
