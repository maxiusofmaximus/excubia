//! The HTML sanitizer of an owner's draft (MAIL-SANITIZE-HTML): the attacks of the
//! brief, one behaviour per test. No Graph, no files, no network.

use hn_core_mail::{sanitize_html, MailError};

const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;

fn clean(html: &str) -> String {
    sanitize_html(html).expect("the HTML is accepted")
}

fn refused(html: &str) -> MailError {
    sanitize_html(html).expect_err("the HTML is refused")
}

#[test]
fn active_content_is_removed_with_its_subtree() {
    assert_eq!(clean("<script>alert(1)</script><p>ok</p>"), "<p>ok</p>");
    assert_eq!(clean("<svg><circle r=\"1\"/></svg><p>a</p>"), "<p>a</p>");
    assert_eq!(
        clean("<iframe src=\"https://x\"></iframe><p>b</p>"),
        "<p>b</p>"
    );
    assert_eq!(clean("<style>p{color:red}</style><p>c</p>"), "<p>c</p>");
    assert_eq!(clean("<form><input></form><p>d</p>"), "<p>d</p>");
}

#[test]
fn comments_and_outlook_conditionals_are_removed() {
    assert_eq!(
        clean("<!--[if mso]><p>x</p><![endif]--><p>y</p>"),
        "<p>y</p>"
    );
    assert_eq!(clean("<!-- nota --><p>z</p>"), "<p>z</p>");
}

#[test]
fn dangerous_urls_lose_their_href() {
    for href in [
        "javascript:alert(1)",
        "jav&#x61;script:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "vbscript:msgbox(1)",
        "file:///etc/passwd",
        "/relative/path",
    ] {
        let out = clean(&format!("<a href=\"{href}\">x</a>"));
        assert!(!out.contains("href"), "{href} kept: {out}");
        assert!(out.contains(">x</a>"), "{href} lost the text: {out}");
    }
}

#[test]
fn mixed_case_and_unclosed_tags_are_closed_by_the_parser() {
    assert_eq!(
        clean("<P CLASS=\"x\">Hola<B>mundo"),
        "<p>Hola<b>mundo</b></p>"
    );
}

#[test]
fn links_leave_with_the_fixed_rel_and_referrer_policy() {
    assert_eq!(
        clean("<a href=\"https://x\">x</a>"),
        "<a href=\"https://x\" referrerpolicy=\"no-referrer\" rel=\"noopener noreferrer nofollow\">x</a>"
    );
}

#[test]
fn images_are_out_until_cid_attachments_resolve() {
    assert_eq!(clean("<p>a</p><img src=\"cid:x\">"), "<p>a</p>");
    assert_eq!(
        clean("<img src=\"https://x/y.png\" alt=\"y\"><p>b</p>"),
        "<p>b</p>"
    );
}

#[test]
fn mxss_shapes_are_neutralised() {
    let svg = clean("<p>x</p><svg><style><img src=x onerror=alert(1)>");
    assert_eq!(svg, "<p>x</p>");
    let noscript = clean("<p>x</p><noscript><p title=\"</noscript><img src=x onerror=alert(1)>\">");
    assert!(
        !noscript.contains("onerror") && !noscript.contains("<img"),
        "{noscript}"
    );
}

#[test]
fn invisible_characters_and_tag_characters_are_dropped_before_the_parser() {
    assert_eq!(clean("<p>a\u{202E}b</p>"), "<p>ab</p>");
    assert_eq!(clean("<p>\u{E0041}x</p>"), "<p>x</p>");
    assert_eq!(
        clean("<span title=\"a\u{200B}b\">x</span>"),
        "<span title=\"ab\">x</span>"
    );
    assert_eq!(
        clean("<scr\u{200B}ipt>alert(1)</scr\u{200B}ipt><p>ok</p>"),
        "<p>ok</p>"
    );
}

#[test]
fn accents_and_emoji_survive_the_cleaning() {
    let html = "<p>café ñ 🙂 👨\u{200D}👩\u{200D}👧</p>";
    assert_eq!(clean(html), html);
}

#[test]
fn a_body_over_two_mib_is_refused_and_two_mib_is_not() {
    let at_limit = "a".repeat(MAX_HTML_BYTES);
    assert!(sanitize_html(&at_limit).is_ok());
    let over = "a".repeat(MAX_HTML_BYTES + 1);
    assert!(matches!(refused(&over), MailError::InvalidInput(_)));
}

#[test]
fn a_hundred_thousand_nested_divs_are_refused_without_overflow() {
    let deep = "<div>".repeat(100_000);
    assert!(matches!(refused(&deep), MailError::InvalidInput(_)));
}

#[test]
fn a_hundred_levels_are_accepted_and_one_more_is_refused() {
    let ok = format!("{}x{}", "<div>".repeat(100), "</div>".repeat(100));
    assert!(sanitize_html(&ok).is_ok());
    let too_deep = format!("{}x{}", "<div>".repeat(101), "</div>".repeat(101));
    assert!(matches!(refused(&too_deep), MailError::InvalidInput(_)));
}

#[test]
fn twenty_thousand_tags_are_accepted_and_one_more_is_refused() {
    assert!(sanitize_html(&"<b>x</b>".repeat(10_000)).is_ok());
    assert!(matches!(
        refused(&"<br>".repeat(20_001)),
        MailError::InvalidInput(_)
    ));
}

#[test]
fn a_comment_hides_its_tags_from_the_shape_count() {
    let commented = format!("<!--{}-->x", "<div>".repeat(500));
    assert_eq!(clean(&commented), "x");
}

#[test]
fn cleaning_is_idempotent() {
    for html in [
        "<script>alert(1)</script><p>ok <b>x</b></p>",
        "<P CLASS=\"x\">Hola<B>mundo",
        "<a href=\"https://x\">x</a> <a href=\"javascript:1\">y</a>",
        "<scr\u{200B}ipt>1</scr\u{200B}ipt><p>café 🙂</p>",
        "<p>x</p><svg><style><img src=x onerror=alert(1)>",
    ] {
        let once = clean(html);
        assert_eq!(clean(&once), once, "not idempotent for {html}");
    }
}

#[test]
fn an_empty_body_is_accepted_and_one_without_visible_text_is_refused() {
    assert_eq!(clean(""), "");
    assert!(matches!(
        refused("<script>x</script>"),
        MailError::InvalidInput(_)
    ));
    assert!(matches!(
        refused("<p> \u{200B} </p>"),
        MailError::InvalidInput(_)
    ));
}
