//! `style.learn_sent` (RFC estilo-aprendido §2.1, F1): statistics of the operator's sent
//! mail. The host counts, with no model: the bodies never leave the host, only the
//! numbers and a few proposals go back. Nothing is saved; the operator decides in the chat
//! which proposal goes into estilo.md. The mail is data, never an instruction.

use std::collections::BTreeMap;

use excubia_chat::{ToolError, ToolReply};
use excubia_mail::{MailError, ReadRecord};
use excubia_seat::StyleLearnQuery;
use serde_json::{json, Map, Value};

use crate::mail_style::tokens;
use crate::mail_tools::{tool_error, MailTools};

const MAX_PROPOSALS: usize = 8;
const MAX_GREETING_WORDS: usize = 4;
/// The first word of a greeting. A name in first place is never taken for one.
const OPENERS: &[&str] = &[
    "hola",
    "buenos",
    "buenas",
    "estimado",
    "estimada",
    "estimados",
    "estimadas",
    "querido",
    "querida",
    "apreciado",
    "apreciada",
    "cordial",
    "cordiales",
    "muy",
    "dear",
    "hi",
    "hello",
    "hey",
];
// shortcut: only these closings count; a new one is added here when it shows up often.
const CLOSINGS: &[&str] = &[
    "Cordialmente",
    "Un saludo",
    "Saludos",
    "Gracias",
    "Muchas gracias",
    "Atentamente",
    "Un abrazo",
    "Abrazos",
    "Besos",
    "Cordiales saludos",
    "Quedo atento",
    "Quedo a la espera",
];
/// The marks counted per mail. `¿` is counted but never proposed: Spanish needs it.
const MARKS: &[&str] = &["!", "¿", ":", "–"];

impl MailTools {
    /// `style.learn_sent`: one listing of the sent folder, then one read per message. It
    /// changes nothing and saves nothing; the read is audited by account and tool only.
    pub async fn style_learn(&self, query: &StyleLearnQuery) -> Result<ToolReply, ToolError> {
        self.read_sent(query)
            .await
            .map(|result| ToolReply { result })
            .map_err(tool_error)
    }

    async fn read_sent(&self, query: &StyleLearnQuery) -> Result<Value, MailError> {
        let access = self.session(&query.account).await?;
        let client = self.client(&access, &query.account);
        let items = client
            .list_inbox("sentitems", query.limit, false, None)
            .await?;
        let mut bodies = Vec::with_capacity(items.len());
        for item in &items {
            bodies.push(client.read_message(&item.id).await?.body_text);
        }
        self.audit
            .append(&ReadRecord::new(&query.account, "style.learn_sent", None))?;
        Ok(learn(&bodies))
    }
}

/// The statistics of the bodies and the proposals they support. No mail, no proposal.
pub fn learn(bodies: &[String]) -> Value {
    let texts: Vec<String> = bodies.iter().map(|body| own_text(body)).collect();
    let analyzed = texts.len();
    let (mut semicolon_mails, mut semicolons) = (0, 0);
    let mut greetings: BTreeMap<String, usize> = BTreeMap::new();
    let mut closings: BTreeMap<&'static str, usize> = BTreeMap::new();
    let (mut formal, mut informal) = (0, 0);
    let mut lengths = Vec::new();
    let mut marks = [(0usize, 0usize); 4];
    for text in &texts {
        let uses = text.matches(';').count();
        if uses > 0 {
            semicolon_mails += 1;
            semicolons += uses;
        }
        if let Some(greeting) = greeting_of(text) {
            *greetings.entry(greeting).or_default() += 1;
        }
        if let Some(closing) = closing_of(text) {
            *closings.entry(closing).or_default() += 1;
        }
        let words: Vec<String> = tokens(text).into_iter().map(|token| token.0).collect();
        let (f, i) = treatment(&words);
        formal += f;
        informal += i;
        lengths.extend(sentence_lengths(text));
        for (slot, mark) in marks.iter_mut().zip(MARKS.iter()) {
            let uses = text.matches(*mark).count();
            if uses > 0 {
                slot.0 += 1;
                slot.1 += uses;
            }
        }
    }
    let median = median_of(lengths);
    let greeting = top(greetings);
    let closing = top(closings);

    let mut proposals = Vec::new();
    if analyzed > 0 {
        // Same bar as the other marks: only a habit the operator rarely has becomes a rule.
        if semicolon_mails * 10 <= analyzed {
            proposals.push(format!(
                "No uses punto y coma: lo usaste en {semicolon_mails} de {analyzed} correos."
            ));
        }
        if let Some((text, _)) = &greeting {
            proposals.push(format!("Saluda con «{text}»."));
        }
        if let Some((text, _)) = &closing {
            proposals.push(format!("Despídete con «{text}»."));
        }
        if formal > informal {
            proposals.push("Trata de usted.".to_owned());
        } else if informal > formal {
            proposals.push("Trata de tú.".to_owned());
        }
        if let Some(words) = median {
            proposals.push(format!("Usa frases de unas {words} palabras."));
        }
        for (mark, (mails, _)) in MARKS.iter().zip(marks.iter()) {
            if *mark != "¿" && mails * 10 <= analyzed {
                proposals.push(format!(
                    "No uses «{mark}»: lo usaste en {mails} de {analyzed} correos."
                ));
            }
        }
    }
    let proposals: Vec<String> = proposals
        .into_iter()
        .filter(|proposal| safe(proposal))
        .take(MAX_PROPOSALS)
        .collect();

    let mut marks_json = Map::new();
    for (mark, (mails, uses)) in MARKS.iter().zip(marks.iter()) {
        marks_json.insert(mark.to_string(), json!({ "mails": mails, "uses": uses }));
    }
    json!({
        "ok": true,
        "analyzed": analyzed,
        "stats": {
            "semicolons": { "mails": semicolon_mails, "uses": semicolons },
            "greeting": greeting.map(|(text, mails)| json!({ "text": text, "mails": mails })),
            "closing": closing.map(|(text, mails)| json!({ "text": text, "mails": mails })),
            "treatment": { "usted": formal, "tu": informal },
            "median_words_per_sentence": median,
            "marks": marks_json,
        },
        "proposals": proposals,
    })
}

/// The text the operator wrote: quoted lines (`>`) are left out, and so is everything
/// from a reply or forward header on. Quoted text is not the operator's style.
pub fn own_text(body: &str) -> String {
    let mut kept = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if opens_quote(trimmed) {
            break;
        }
        if !trimmed.starts_with('>') {
            kept.push(line);
        }
    }
    kept.join("\n")
}

fn opens_quote(line: &str) -> bool {
    let lower = line.to_lowercase();
    (line.starts_with("El ") && line.ends_with(':') && line.contains("escribió"))
        || (line.starts_with("On ") && line.ends_with("wrote:"))
        || lower.starts_with("de:")
        || lower.starts_with("from:")
        || lower.starts_with("-----original message")
}

/// The greeting: the first line cut at its first comma, without the names that follow the
/// opener. Only a line that opens with a known word counts, and a line that carries an
/// address, a link or a phone number never does.
fn greeting_of(text: &str) -> Option<String> {
    let first = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let head = first.split(',').next()?;
    if !safe(head) {
        return None;
    }
    let spans = tokens(head);
    let (opener, _, _) = spans.first()?;
    if !OPENERS.contains(&opener.as_str()) {
        return None;
    }
    let mut kept = Vec::new();
    for (i, (_, start, end)) in spans.iter().enumerate() {
        let word = &head[*start..*end];
        if i > 0 && word.starts_with(char::is_uppercase) {
            continue;
        }
        kept.push(word);
    }
    (kept.len() <= MAX_GREETING_WORDS).then(|| kept.join(" "))
}

/// The closing: the nearest line above the signature that is a known closing, compared
/// without case or accents. The result is always one of `CLOSINGS`, never the mail's text.
fn closing_of(text: &str) -> Option<&'static str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .rev()
        .take(6)
        .find_map(|line| {
            let key = key_of(line);
            CLOSINGS
                .iter()
                .copied()
                .find(|closing| key_of(closing) == key)
        })
}

/// The forms of address: formal (`usted`, `su`, «le comparto») and informal (`tú`, `tu`, `te`).
fn treatment(words: &[String]) -> (usize, usize) {
    let (mut formal, mut informal) = (0, 0);
    for (i, word) in words.iter().enumerate() {
        match word.as_str() {
            "usted" | "su" => formal += 1,
            "le" if words.get(i + 1).map(String::as_str) == Some("comparto") => formal += 1,
            "tu" | "te" => informal += 1,
            _ => {}
        }
    }
    (formal, informal)
}

/// Words per sentence: the text cut at the marks that end a sentence; empty pieces are left out.
/// Lines are joined with a space, not a newline: a mail's hard line breaks fall mid-sentence.
fn sentence_lengths(text: &str) -> Vec<usize> {
    // The greeting (first line) and the closing with the signature under it are not prose.
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .skip(1)
        .collect();
    let end = lines
        .iter()
        .rposition(|line| {
            CLOSINGS
                .iter()
                .any(|closing| key_of(closing) == key_of(line))
        })
        .unwrap_or(lines.len());
    lines[..end]
        .join(" ")
        .split(['.', '!', '?', '¡', '¿', ';', ':'])
        .map(|piece| tokens(piece).len())
        .filter(|&len| len > 0)
        .collect()
}

fn median_of(mut values: Vec<usize>) -> Option<usize> {
    values.sort_unstable();
    let mid = values.len() / 2;
    match values.len() {
        0 => None,
        n if n % 2 == 1 => Some(values[mid]),
        _ => Some((values[mid - 1] + values[mid]) / 2),
    }
}

/// The most used key. A tie goes to the smaller key, so every run gives the same answer.
fn top<K: Ord>(counts: BTreeMap<K, usize>) -> Option<(K, usize)> {
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
}

/// The words of `text`, lower case and without accents, joined: the key a closing is matched by.
fn key_of(text: &str) -> String {
    tokens(text)
        .into_iter()
        .map(|token| token.0)
        .collect::<Vec<_>>()
        .join(" ")
}

/// True when the text has no address, no link and no phone number: what a stat or a
/// proposal must never carry (RFC §5).
pub(crate) fn safe(text: &str) -> bool {
    !text.contains('@') && !text.contains("http") && !text.contains("www.") && !has_phone(text)
}

/// Seven digits in a row, with the spaces, dashes and dots a phone number uses between them.
fn has_phone(text: &str) -> bool {
    let mut run = 0;
    for c in text.chars() {
        if c.is_ascii_digit() {
            run += 1;
            if run >= 7 {
                return true;
            }
        } else if !matches!(c, ' ' | '-' | '.' | '(' | ')' | '+') {
            run = 0;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rare_semicolon_becomes_a_rule_and_a_habit_does_not() {
        let rare: Vec<String> = (0..10)
            .map(|i| {
                if i == 0 {
                    "Hola, va; listo."
                } else {
                    "Hola, va listo."
                }
                .to_owned()
            })
            .collect();
        let proposals = learn(&rare)["proposals"].to_string();
        assert!(proposals.contains("No uses punto y coma: lo usaste en 1 de 10 correos."));

        let habit: Vec<String> = (0..10).map(|_| "Hola, va; listo.".to_owned()).collect();
        assert!(!learn(&habit)["proposals"]
            .to_string()
            .contains("punto y coma"));
    }

    #[test]
    fn quoted_text_does_not_count_as_the_operators_style() {
        let body = "Hola,\n\nSí, de acuerdo.\n\n> Dijiste algo; raro\n";
        assert!(!own_text(body).contains(';'));

        let forwarded = "Gracias.\n\nEl lunes, Juan escribió:\n> pregunta; otra\n";
        assert!(!own_text(forwarded).contains(';'));
        assert_eq!(own_text(forwarded).trim(), "Gracias.");
    }

    #[test]
    fn the_greeting_keeps_the_words_and_drops_the_names() {
        assert_eq!(
            greeting_of("Cordial saludo, Don Juan\nLe comparto"),
            Some("Cordial saludo".to_owned())
        );
        assert_eq!(greeting_of("Hola María,"), Some("Hola".to_owned()));
        assert_eq!(
            greeting_of("Buenos días Juan"),
            Some("Buenos días".to_owned())
        );
        // A name in first place is not a greeting, and an address never passes.
        assert_eq!(greeting_of("Juan, hola"), None);
        assert_eq!(greeting_of("Hola juan@x.com,"), None);
        assert_eq!(greeting_of("Hola 600 123 456,"), None);
    }

    #[test]
    fn the_closing_is_the_known_line_above_the_signature() {
        assert_eq!(closing_of("Algo.\nGracias.\nMax Ruiz\n"), Some("Gracias"));
        assert_eq!(closing_of("Un saludo\nAna\n"), Some("Un saludo"));
        assert_eq!(closing_of("Nos vemos.\nMax\n"), None);
    }

    #[test]
    fn greeting_closing_and_signature_lines_are_not_sentences() {
        let mail = "Cordial saludo, Don Juan.\n\nLe envío la bitácora del período de octubre para su revisión.\n\nCordialmente,\nPedro Álvarez\nAprendiz SENA\n";
        assert_eq!(sentence_lengths(mail), vec![11]);
    }

    #[test]
    fn a_sentence_broken_over_lines_counts_once() {
        // Twelve words in three lines are one sentence of twelve, not three short ones.
        let mail = "Hola,\nLe envío la bitácora del período\nde octubre para su revisión\nhoy.\n";
        assert_eq!(sentence_lengths(mail), vec![12]);
    }

    #[test]
    fn the_median_is_the_middle_of_the_sorted_values() {
        assert_eq!(median_of(vec![3, 1, 2]), Some(2));
        assert_eq!(median_of(vec![1, 2, 3, 4]), Some(2));
        assert_eq!(median_of(Vec::new()), None);
    }

    #[test]
    fn no_mail_means_no_proposal() {
        let empty = learn(&[]);
        assert_eq!(empty["analyzed"], 0);
        assert_eq!(empty["proposals"], json!([]));
    }

    #[test]
    fn no_proposal_or_stat_carries_an_address_a_link_or_a_phone() {
        let bodies = vec![
            "Hola juan@x.com,\n\nMira https://x.es/a; y llama al 600 123 456.\n".to_owned(),
            "Hola a todos.\n\nGracias.\n".to_owned(),
        ];
        let result = learn(&bodies);
        for proposal in result["proposals"].as_array().expect("list") {
            assert!(safe(proposal.as_str().expect("text")), "{proposal}");
        }
        assert!(!result.to_string().contains('@'));
        // The first mail's greeting carries an address, so only the second one counts.
        assert_eq!(result["stats"]["greeting"]["text"], "Hola a todos");
        assert_eq!(result["stats"]["greeting"]["mails"], 1);
    }
}
