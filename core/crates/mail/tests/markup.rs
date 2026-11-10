//! The text the operator reads from an HTML body another client wrote: tags
//! dropped, entities decoded once, a bare `<` kept as text.

use excubia_core_mail::DraftBody;

#[test]
fn other_html_keeps_its_lines_and_loses_its_markup() {
    let html = "<html><head><title>x</title><style>p{color:red}</style></head>\n\
                <body>\n<p>Hola<br>Ana</p>\n\
                <div>Fin&nbsp;del&#32;mensaje &#8364;</div><!-- oculto --></body></html>";
    assert_eq!(
        DraftBody::Html(html.to_owned()).text(),
        "Hola\nAna\nFin del mensaje €"
    );
}

#[test]
fn table_cells_and_rows_end_their_lines_as_blocks_do() {
    let html = "<table><tr><td><h1>Título</h1></td></tr><tr><td>Uno</td><td>Dos</td></tr></table>";
    assert_eq!(DraftBody::Html(html.to_owned()).text(), "Título\nUno\nDos");
}

#[test]
fn a_bare_less_than_sign_is_text_and_an_entity_is_decoded_once() {
    assert_eq!(
        DraftBody::Html("a < b &amp;lt; c".to_owned()).text(),
        "a < b &lt; c"
    );
}
