//! The text sanitizer (MAIL-SANITIZE-TEXT): the acceptance cases of the brief,
//! one behaviour per test. No Graph, no files, no network.

use excubia_mail::{sanitize_text, MailError, Policy, Sanitized};

fn outgoing(text: &str) -> Sanitized {
    sanitize_text(text, Policy::Outgoing).expect("the text is accepted")
}

fn incoming(text: &str) -> Sanitized {
    sanitize_text(text, Policy::Incoming).expect("the text is accepted")
}

fn rejected(text: &str, policy: Policy) -> MailError {
    sanitize_text(text, policy).expect_err("the text is rejected")
}

#[test]
fn ascii_and_spanish_text_is_untouched() {
    for text in [
        "Hola, mundo.",
        "Pedro Álvarez, ñ, ü, ¿¡?!",
        "Comillas “tipográficas”, ‘simples’, guion — y puntos suspensivos …",
        "Línea uno\nLínea dos\tcon tabulador",
    ] {
        let out = outgoing(text);
        assert_eq!(out.text, text);
        assert_eq!(out.removed, 0);
    }
}

#[test]
fn invisible_characters_are_removed() {
    let out = outgoing("a\u{200B}b\u{FEFF}c\u{2060}d\u{00AD}e");

    assert_eq!(out.text, "abcde");
    assert_eq!(out.removed, 4);
}

#[test]
fn tags_outside_a_flag_are_removed() {
    let out = outgoing("Hola\u{E0048}\u{E0069}\u{E007F}");

    assert_eq!(out.text, "Hola");
    assert_eq!(out.removed, 3);
}

#[test]
fn bidi_controls_are_removed() {
    let out = outgoing("a\u{202E}b\u{2066}c\u{2067}d\u{2068}e\u{2069}f");

    assert_eq!(out.text, "abcdef");
    assert_eq!(out.removed, 5);
}

#[test]
fn stray_variation_selectors_are_removed() {
    assert_eq!(outgoing("a\u{FE0F}").text, "a");
    assert_eq!(outgoing("❤\u{FE0E}").text, "❤");
    assert_eq!(outgoing("x\u{E0100}y").text, "xy");
}

#[test]
fn valid_emoji_sequences_survive() {
    for sequence in [
        "👨\u{200D}👩\u{200D}👧",
        "🏳\u{FE0F}\u{200D}🌈",
        "1\u{FE0F}\u{20E3}",
        "👍🏽",
        "🏴\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}",
        "🇪🇸",
        "❤\u{FE0F}",
        "👩🏽\u{200D}💻",
    ] {
        let out = outgoing(sequence);
        assert_eq!(out.text, sequence);
        assert_eq!(out.removed, 0);
    }
}

#[test]
fn loose_joiners_and_selectors_are_removed() {
    assert_eq!(outgoing("a\u{200D}b").text, "ab");
    assert_eq!(outgoing("👨\u{200D}").text, "👨");
    assert_eq!(outgoing("👨\u{200D}a").text, "👨a");
    assert_eq!(outgoing("a\u{FE0F}").text, "a");
    let error = rejected("\u{200D}", Policy::Outgoing);
    assert!(error.to_string().contains("no visible content"), "{error}");
}

#[test]
fn an_invalid_tag_flag_loses_its_tags() {
    let no_letter = "🏴\u{E0067}\u{E0062}\u{E0043}\u{E007F}";
    assert_eq!(outgoing(no_letter).text, "🏴");

    let no_terminator = "🏴\u{E0067}\u{E0062}";
    assert_eq!(outgoing(no_terminator).text, "🏴");
}

#[test]
fn c0_and_c1_controls_are_removed_but_tab_and_newline_stay() {
    let out = outgoing("a\u{0001}b\u{0007}c\u{0085}d\tE\nF");

    assert_eq!(out.text, "abcd\tE\nF");
    assert_eq!(out.removed, 3);
}

#[test]
fn crlf_and_cr_become_lf() {
    assert_eq!(outgoing("a\r\nb\rc").text, "a\nb\nc");
}

#[test]
fn line_separators_become_lf() {
    assert_eq!(outgoing("a\u{2028}b\u{2029}c").text, "a\nb\nc");
}

#[test]
fn unassigned_characters_are_dropped_and_counted() {
    let out = outgoing("a\u{0378}b");

    assert_eq!(out.text, "ab");
    assert_eq!(out.removed, 1);
}

#[test]
fn private_use_and_noncharacters_are_rejected() {
    for text in ["a\u{E000}b", "a\u{FDD0}b", "a\u{FFFE}b", "a\u{1FFFF}b"] {
        let error = rejected(text, Policy::Outgoing);
        assert_eq!(error.code(), "mail_invalid_input");
    }
}

#[test]
fn nul_is_rejected_and_never_echoed() {
    let error = rejected("a\0b", Policy::Outgoing);

    assert_eq!(error.code(), "mail_invalid_input");
    assert!(!error.to_string().contains('\0'));
}

#[test]
fn an_error_never_echoes_the_content() {
    let hostile = "\u{FDD0}secreto@example.com";
    let message = rejected(hostile, Policy::Outgoing).to_string();

    assert!(!message.contains("secreto"), "{message}");
    assert!(!message.contains('\u{FDD0}'), "{message}");
}

#[test]
fn text_over_one_mib_is_rejected() {
    let huge = "a".repeat(2 * 1024 * 1024);
    let error = rejected(&huge, Policy::Outgoing);

    assert_eq!(error.code(), "mail_invalid_input");
}

#[test]
fn text_without_visible_content_is_rejected() {
    for text in ["\u{200B}\u{FEFF}\u{2060}", " \n\t ", "\u{00A0}", "\u{034F}"] {
        let error = rejected(text, Policy::Outgoing);
        assert!(
            error.to_string().contains("no visible content"),
            "an invisible text was accepted"
        );
    }
}

#[test]
fn an_empty_text_is_accepted() {
    for policy in [Policy::Outgoing, Policy::Incoming] {
        let out = sanitize_text("", policy).expect("an empty text is the writer's choice");
        assert_eq!(out.text, "");
        assert_eq!(out.removed, 0);
    }
}

#[test]
fn eight_combining_marks_pass_and_nine_are_rejected() {
    let eight = format!("a{}", "\u{0338}".repeat(8));
    assert!(sanitize_text(&eight, Policy::Outgoing).is_ok());

    let nine = format!("a{}", "\u{0338}".repeat(9));
    let error = rejected(&nine, Policy::Outgoing);
    assert!(error.to_string().contains("combining marks"), "{error}");
}

#[test]
fn text_limits_are_enforced() {
    let too_long = "a".repeat(100_001);
    assert!(sanitize_text(&too_long, Policy::Outgoing).is_err());

    let too_many_lines = "a\n".repeat(10_000);
    assert!(sanitize_text(&too_many_lines, Policy::Outgoing).is_err());

    let line_too_long = "a".repeat(10_001);
    assert!(sanitize_text(&line_too_long, Policy::Outgoing).is_err());
}

#[test]
fn spaces_follow_the_policy() {
    let text = "a\u{00A0}b\u{2003}c";

    let owner = outgoing(text);
    assert_eq!(owner.text, text);
    assert_eq!(owner.removed, 0);

    let foreign = incoming(text);
    assert_eq!(foreign.text, "a b c");
    assert_eq!(foreign.removed, 2);
}

#[test]
fn nfc_composes_what_it_can() {
    let out = outgoing("cafe\u{0301}");

    assert_eq!(out.text, "café");
    assert_eq!(out.removed, 0);
}

#[test]
fn the_emoji_table_is_pinned_to_a_unicode_version() {
    assert_eq!(excubia_mail::sanitize::UNICODE_EMOJI_VERSION, "16.0");
}

#[test]
fn sanitizing_twice_changes_nothing() {
    let corpus = [
        "Hola, mundo.",
        "Pedro Álvarez, ñ, ü, ¿¡?!",
        "a\u{200B}b\u{FEFF}c\u{00AD}d",
        "Hola\u{E0048}\u{E0069}\u{E007F}",
        "a\u{202E}b\u{2066}c\u{2069}d",
        "a\u{FE0F}",
        "👨\u{200D}👩\u{200D}👧",
        "🏳\u{FE0F}\u{200D}🌈",
        "1\u{FE0F}\u{20E3}",
        "👍🏽",
        "🏴\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}",
        "🏴\u{E0067}\u{E0062}\u{E0043}\u{E007F}",
        "🇪🇸",
        "❤\u{FE0F}",
        "a\r\nb\rc",
        "a\u{2028}b",
        "a\u{00A0}b\u{2003}c",
        "cafe\u{0301}",
        "a\u{0378}b",
        "a\u{0001}b\tc\nd",
    ];

    for policy in [Policy::Outgoing, Policy::Incoming] {
        for text in corpus {
            let first = sanitize_text(text, policy).expect("the corpus is acceptable");
            let second = sanitize_text(&first.text, policy).expect("the clean text is acceptable");
            assert_eq!(second.text, first.text, "the second pass changed the text");
            assert_eq!(
                second.removed, 0,
                "the second pass removed something from clean text"
            );
        }
    }
}
