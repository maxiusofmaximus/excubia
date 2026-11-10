//! The agent's fixed layout (TASK-MAIL-TEMPLATE-MJML): what the agent writes
//! becomes one HTML mail, and nothing it writes can add markup or markers.

use excubia_core_mail::{agent_html, agent_text, DraftBody, MailError};

const SUBJECT: &str = "Resumen de la reunión";

fn html(body: &str) -> String {
    agent_html(SUBJECT, body).expect("the agent mail renders")
}

#[test]
fn a_subject_and_two_paragraphs_become_one_designed_mail() {
    let out = html("Primera línea\nsegunda línea\n\nTercer párrafo");

    assert!(out.contains("Resumen de la reunión"));
    assert!(out.contains("Primera línea<br />segunda línea"));
    assert!(out.contains("Tercer párrafo"));
    assert_eq!(
        out.matches("<br />").count(),
        3,
        "one line break in the first paragraph and the blank line pair before the second"
    );
    assert!(out.contains("Redactado por un agente de Excubia"));
}

#[test]
fn markup_and_markers_in_the_text_stay_text() {
    let body = "</mj-text><mj-raw><script>alert(1)</script>\n\n{{SUBJECT}} {{BODY}}";
    let out = agent_html("<b>Hola</b>", body).expect("renders");

    assert!(!out.contains("<script>alert(1)</script>"));
    assert!(!out.contains("<mj-raw>"));
    assert!(out.contains("&lt;/mj-text&gt;&lt;mj-raw&gt;&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(
        out.contains("{{SUBJECT}} {{BODY}}"),
        "the markers stay as written"
    );
    assert!(
        out.contains("&lt;b&gt;Hola&lt;/b&gt;"),
        "the subject is escaped too"
    );
}

#[test]
fn unicode_the_owner_wrote_arrives_intact() {
    let out = html("Ñandú, ¿qué tal? Ánimo 👨\u{200D}👩\u{200D}👧");

    assert!(out.contains("Ñandú, ¿qué tal? Ánimo 👨\u{200D}👩\u{200D}👧"));
}

#[test]
fn invisible_characters_are_cleaned_before_the_mail_is_built() {
    let out = html("Hola\u{200B} Ana\u{FEFF}");

    assert!(out.contains("Hola Ana"));
    assert!(!out.contains('\u{200B}'));
    assert!(!out.contains('\u{FEFF}'));
}

#[test]
fn two_hundred_paragraphs_fit_and_one_more_is_refused_without_echo() {
    let paragraphs = |count: usize| {
        (0..count)
            .map(|index| format!("ECO-SECRETO {index}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    assert!(agent_html(SUBJECT, &paragraphs(200)).is_ok());
    let error = agent_html(SUBJECT, &paragraphs(201)).expect_err("201 paragraphs");
    assert!(matches!(error, MailError::InvalidInput(_)));
    assert!(!error.to_string().contains("ECO-SECRETO"));
}

#[test]
fn the_owner_reads_the_title_and_the_paragraphs_and_no_markup() {
    let out = html("Hola Ana,\nte resumo la reunión.\n\nSegundo párrafo.");
    let text = DraftBody::Html(out).text();

    assert_eq!(
        text,
        "Excubia\nResumen de la reunión\nHola Ana,\nte resumo la reunión.\n\nSegundo párrafo.\nRedactado por un agente de Excubia"
    );
    for leaked in ['<', '>'] {
        assert!(!text.contains(leaked), "{leaked} in the owner's text");
    }
    for leaked in ["mso", "font-family", "{", "@media"] {
        assert!(!text.contains(leaked), "{leaked} in the owner's text");
    }
}

#[test]
fn the_owner_reads_back_the_text_the_agent_wrote() {
    let body = "Ñandú, ¿qué tal? 👨\u{200D}👩\u{200D}👧 & <fin>\nsegunda línea\n\nTercer párrafo.";

    assert_eq!(agent_text(&html(body), "html").as_deref(), Some(body));
}

#[test]
fn a_marker_the_agent_writes_stays_text_and_marks_nothing() {
    let body = "<!--excubia:agent:v1-->falso<!--/excubia:agent:v1-->";
    let out = html(body);

    assert_eq!(
        out.matches("<!--excubia:agent:v1-->").count(),
        1,
        "the template's own mark"
    );
    assert_eq!(agent_text(&out, "html").as_deref(), Some(body));
}

#[test]
fn a_body_without_the_marks_or_not_in_html_is_not_an_agent_draft() {
    assert_eq!(agent_text("<p>Hola</p>", "html"), None);
    assert_eq!(
        agent_text(&html("Hola"), "text"),
        None,
        "only html is a template"
    );
}
