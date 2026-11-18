//! Message text validation shared by the HTTP routes and the MCP tools. The
//! two transports shape errors differently, so this returns a typed reason and
//! lets each boundary map it (`orchestrator-chat.md` §3, §3.3).

use thiserror::Error;

pub const MAX_MESSAGE_BYTES: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum TextError {
    #[error("text is required")]
    Empty,
    #[error("text contains control characters")]
    ControlCharacters,
    #[error("text cannot exceed {MAX_MESSAGE_BYTES} bytes")]
    TooLong,
}

/// Trim, reject empty/control text and enforce the 32 KiB UTF-8 bound. The
/// returned string is what gets stored, so every writer applies the same rule.
pub fn message_text(raw: &str) -> Result<String, TextError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(TextError::Empty);
    }
    if text
        .chars()
        .any(|character| character.is_control() && character != '\n' && character != '\t')
    {
        return Err(TextError::ControlCharacters);
    }
    if text.len() > MAX_MESSAGE_BYTES {
        return Err(TextError::TooLong);
    }
    Ok(text.to_string())
}

pub const MAX_NAME_CHARS: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum NameError {
    #[error("name is required")]
    Empty,
    #[error("name cannot exceed {MAX_NAME_CHARS} characters")]
    TooLong,
    #[error("name contains control characters")]
    ControlCharacters,
}

/// Trim and bound a display name: 1 to 40 characters and no control character,
/// tab or newline included (a name is one line).
pub fn agent_name(raw: &str) -> Result<String, NameError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.chars().any(char::is_control) {
        return Err(NameError::ControlCharacters);
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(NameError::TooLong);
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_trimmed_and_bounded() {
        assert_eq!(message_text("  hola  ").expect("text"), "hola");
        assert_eq!(message_text("   "), Err(TextError::Empty));
        assert_eq!(message_text("a\u{7}"), Err(TextError::ControlCharacters));
        assert_eq!(
            message_text(&"a".repeat(MAX_MESSAGE_BYTES + 1)),
            Err(TextError::TooLong)
        );
        assert_eq!(
            message_text(&"a".repeat(MAX_MESSAGE_BYTES))
                .expect("at the limit")
                .len(),
            MAX_MESSAGE_BYTES
        );
    }

    #[test]
    fn names_are_trimmed_one_line_and_bounded() {
        assert_eq!(agent_name("  Ines  ").expect("name"), "Ines");
        assert_eq!(agent_name(" \n "), Err(NameError::Empty));
        assert_eq!(agent_name("Ines\tRíos"), Err(NameError::ControlCharacters));
        assert_eq!(agent_name("Ines\u{7}"), Err(NameError::ControlCharacters));
        assert_eq!(
            agent_name(&"ñ".repeat(MAX_NAME_CHARS + 1)),
            Err(NameError::TooLong)
        );
        assert_eq!(
            agent_name(&"ñ".repeat(MAX_NAME_CHARS))
                .expect("at the limit")
                .chars()
                .count(),
            MAX_NAME_CHARS
        );
    }
}

/// The prefixes of the identifiers the host mints with `new_id`.
const INTERNAL_PREFIXES: [&str; 22] = [
    "ag", "apr", "ar", "att", "cc", "cnv", "co", "ct", "dlg", "ev", "hire", "mcd", "mdc", "msg",
    "nt", "plan", "prj", "rcp", "run", "sch", "sec", "srn",
];
/// The shortest body of an identifier after its `_`. Real ones have 26 characters.
const MIN_ID_BODY: usize = 20;
/// The words that name an identifier in a sentence («tarea dlg_…», «(delegación dlg_…)»).
const ID_LABELS: [&str; 4] = ["tarea", "delegación", "delegacion", "id"];

/// Take the internal identifiers out of a text Alfred writes for the user, with the
/// wrapper that names them («Quedó en cola (tarea dlg_…).»). A text without one comes
/// back unchanged. An identifier starts a word and ends where no word character follows,
/// so `flag_x1` and other words that only contain a prefix stay.
pub fn without_internal_ids(text: &str) -> String {
    let spans = internal_id_spans(text);
    if spans.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, end) in spans {
        let (lo, hi) = spacing(text, wrapper(text, start, end));
        let lo = lo.max(cursor);
        let hi = hi.max(lo);
        out.push_str(&text[cursor..lo]);
        cursor = hi;
    }
    out.push_str(&text[cursor..]);
    out.trim().to_string()
}

fn is_word_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// The byte ranges of the identifiers in `text`, in order.
fn internal_id_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut free = 0;
    for (start, _) in text.char_indices() {
        if start < free || text[..start].chars().next_back().is_some_and(is_word_char) {
            continue;
        }
        let rest = &text[start..];
        let length = INTERNAL_PREFIXES.iter().find_map(|prefix| {
            let body = rest.strip_prefix(prefix)?.strip_prefix('_')?;
            let len = body.bytes().take_while(u8::is_ascii_alphanumeric).count();
            let word_ends = body[len..].chars().next().is_none_or(|c| !is_word_char(c));
            (len >= MIN_ID_BODY && word_ends).then_some(prefix.len() + 1 + len)
        });
        if let Some(length) = length {
            spans.push((start, start + length));
            free = start + length;
        }
    }
    spans
}

/// The range an identifier takes with it: its parentheses, or the label that names it.
fn wrapper(text: &str, start: usize, end: usize) -> (usize, usize) {
    let head = text[..start].trim_end_matches(' ');
    let label = label_before(head);
    let name_start = label.unwrap_or(head.len());
    let before = text[..name_start].trim_end_matches(' ');
    if let Some(open) = before.strip_suffix('(').map(str::len) {
        let tail = text[end..].trim_start_matches(' ');
        if tail.starts_with(')') {
            return (open, text.len() - tail.len() + 1);
        }
    }
    (label.unwrap_or(start), end)
}

/// Where the label that names an identifier starts, when `head` ends with one.
fn label_before(head: &str) -> Option<usize> {
    ID_LABELS.iter().find_map(|label| {
        let start = head.len().checked_sub(label.len())?;
        let word = head.get(start..)?;
        let inside_word = head[..start]
            .chars()
            .next_back()
            .is_some_and(char::is_alphabetic);
        (word.to_lowercase() == *label && !inside_word).then_some(start)
    })
}

/// Takes one space with the removed text, so the sentence keeps a single space and no
/// space before a mark of punctuation.
fn spacing(text: &str, (lo, hi): (usize, usize)) -> (usize, usize) {
    let after = text[hi..].chars().next();
    let space_before = text[..lo].ends_with(' ');
    if space_before
        && after.is_none_or(|c| {
            c == ' ' || matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | '»' | '…')
        })
    {
        return (lo - 1, hi);
    }
    if after == Some(' ')
        && text[..lo]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace)
    {
        return (lo, hi + 1);
    }
    (lo, hi)
}

/// The most characters an approval headline keeps.
pub const MAX_HEADLINE_CHARS: usize = 120;

/// The one-line description a tool gives its action (a shell command's `description`), as
/// plain text: controls and runs of blanks become one space, and the text is cut to
/// 120 characters, «…» included when it is cut. `None` when nothing readable is left.
pub fn headline_of(raw: &str) -> Option<String> {
    let flat: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if flat.is_empty() {
        return None;
    }
    if flat.chars().count() <= MAX_HEADLINE_CHARS {
        return Some(flat);
    }
    let cut: String = flat.chars().take(MAX_HEADLINE_CHARS - 1).collect();
    Some(format!("{}…", cut.trim_end()))
}

#[cfg(test)]
mod id_tests {
    use super::*;

    const ID: &str = "dlg_01M7JMD9JNE1NYT6WNFVCGEXBD";
    const OTHER: &str = "apr_01M7JMD9JNE1NYT6WNFVCGEXBE";

    #[test]
    fn a_bare_id_goes_and_the_sentence_keeps_its_spacing() {
        assert_eq!(
            without_internal_ids(&format!("{ID} queda en cola.")),
            "queda en cola."
        );
        assert_eq!(
            without_internal_ids(&format!("Listo, {ID} sigue.")),
            "Listo, sigue."
        );
        assert_eq!(without_internal_ids(&format!("Creé {ID}.")), "Creé.");
        assert_eq!(
            without_internal_ids("Hilo cnv_0123456789ABCDEFGHIJKLMNOP listo."),
            "Hilo listo."
        );
    }

    #[test]
    fn a_parenthesised_id_takes_its_parentheses() {
        assert_eq!(
            without_internal_ids(&format!("Quedó en cola ({ID}).")),
            "Quedó en cola."
        );
        assert_eq!(
            without_internal_ids(&format!("Hecho ({ID}), dos.")),
            "Hecho, dos."
        );
    }

    #[test]
    fn a_label_before_the_id_goes_with_it() {
        assert_eq!(
            without_internal_ids(&format!("Quedó en cola (tarea {ID}).")),
            "Quedó en cola."
        );
        assert_eq!(
            without_internal_ids(&format!("Quedó en cola (delegación {ID}).")),
            "Quedó en cola."
        );
        assert_eq!(
            without_internal_ids(&format!("Quedó en cola (id {ID}).")),
            "Quedó en cola."
        );
    }

    #[test]
    fn several_ids_in_one_sentence_all_go() {
        assert_eq!(
            without_internal_ids(&format!("Creé {ID} y {OTHER}.")),
            "Creé y."
        );
    }

    #[test]
    fn a_text_without_ids_is_returned_unchanged() {
        let text = "Hola  mundo, la tarea está hecha.";
        assert_eq!(without_internal_ids(text), text);
    }

    #[test]
    fn words_that_only_look_like_ids_stay() {
        let text = "ag_bob y flag_x1 quedan, y run_loop_counter también.";
        assert_eq!(without_internal_ids(text), text);
        let longer = format!("{ID}_extra");
        assert_eq!(without_internal_ids(&longer), longer);
    }

    #[test]
    fn a_headline_is_plain_one_line_and_bounded() {
        assert_eq!(
            headline_of("  Crear\nredi-uno.txt\t con el texto uno "),
            Some("Crear redi-uno.txt con el texto uno".to_string())
        );
        assert_eq!(headline_of(" \n\u{7} "), None);
        let cut = headline_of(&"ñ".repeat(200)).expect("headline");
        assert_eq!(cut.chars().count(), MAX_HEADLINE_CHARS);
        assert!(cut.ends_with('…'));
    }
}

/// The title a conversation gets from its first message while it is still «Nuevo chat»:
/// the first line, without Markdown marks, cut at a word so that the title, «…»
/// included when it is cut, stays within 40 characters. `None` when nothing readable
/// is left (a message that is only an attachment).
pub fn title_from(text: &str) -> Option<String> {
    const MAX: usize = 40;
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let clean: String = line
        .chars()
        .filter(|c| !matches!(c, '*' | '_' | '`' | '#' | '>' | '[' | ']'))
        .collect();
    let body = without_request_frame(&clean);
    let body = if body.trim().chars().count() < 3 {
        clean.as_str()
    } else {
        body
    };
    let summary = verb_summary(body);
    let clause = if summary.is_some() { body } else { first_clause(body) };
    let source = summary.as_deref().unwrap_or(clause);
    let words: Vec<&str> = source.split_whitespace().collect();
    // A first clause that is a little long is cut at a word with no «…»: it already ends a thought.
    let by_clause = summary.is_none() && clause.len() < body.len();
    let cut = !by_clause && words.join(" ").chars().count() > MAX;
    let budget = if cut { MAX - 1 } else { MAX };
    let mut title = String::new();
    for word in words {
        let next = if title.is_empty() {
            word.chars().count()
        } else {
            title.chars().count() + 1 + word.chars().count()
        };
        if next > budget {
            if title.is_empty() {
                title = word.chars().take(budget).collect();
            }
            break;
        }
        if !title.is_empty() {
            title.push(' ');
        }
        title.push_str(word);
    }
    if cut {
        title.push('…');
    }
    if by_clause {
        // Never end on a linking word («… está el»): drop it.
        while let Some((head, last)) = title.rsplit_once(' ') {
            if !is_link(last) {
                break;
            }
            title = head.to_string();
        }
    }
    let title = title
        .trim_end_matches(['.', ',', ';', ':', '?', '!', '¿', '¡'])
        .to_string();
    let mut chars = title.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

/// A request with no known verb is long: its first clause («en qué rama está el proyecto, cuáles
/// son…» gives «en qué rama está el proyecto») reads better than the whole cut by «…». A clause
/// of fewer than 12 or more than 70 characters is no title, so the text stays whole.
fn first_clause(text: &str) -> &str {
    match text.split_once(", ") {
        Some((head, _)) if (12..=70).contains(&head.chars().count()) => head,
        _ => text,
    }
}

/// The task's title for a sentence the host writes: no paths (only the file name stays),
/// no ids, at most 40 characters. A task with no title rule takes its first 40 characters.
pub fn close_title(task: &str) -> String {
    let title = title_from(task).unwrap_or_else(|| task.trim().chars().take(40).collect());
    title
        .split_whitespace()
        .map(|word| word.rsplit(['/', '\\']).next().unwrap_or_default())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The wrapper around a request («Pídele a X que», «Por favor,»), so the title keeps the task.
fn without_request_frame(text: &str) -> &str {
    let mut rest = text.trim_start();
    while let Some(next) = strip_frame(rest) {
        rest = next.trim_start();
    }
    rest
}

fn strip_frame(text: &str) -> Option<&str> {
    const FRAMES: [&str; 8] = [
        "Necesito que ",
        "Quiero que ",
        "Por favor, ",
        "¿Puedes ",
        "Puedes ",
        "Me diga ",
        "Me digas ",
        "Dime ",
    ];
    FRAMES
        .iter()
        .find_map(|frame| strip_ci(text, frame))
        .or_else(|| strip_order(text))
        .or_else(|| strip_commission(text))
        .or_else(|| strip_endorsement(text))
        .or_else(|| strip_lead_in(text))
        .or_else(|| strip_place(text))
}

/// `En la carpeta del proyecto Bench (bench-proj), lista …`: a place of up to ten words closed
/// by a comma tells where, not what. What follows is the task.
fn strip_place(text: &str) -> Option<&str> {
    let (place, task) = strip_ci(text, "En ")?.split_once(", ")?;
    let short = place.split_whitespace().count() <= 10 && !place.contains(['.', ':']);
    // «En qué rama está…, cuáles…» is a question, not a place.
    let question = place
        .split_whitespace()
        .next()
        .is_some_and(|word| matches!(plain(word).as_str(), "que" | "quien" | "cual" | "cuales" | "como" | "cuando" | "donde" | "cuanto"));
    (short && !question && task.trim().chars().count() >= 3).then_some(task)
}

/// `Con un solo comando de shell: muestra …`: a short lead-in of at most six words closed by
/// a colon and a space, with no sentence end before it. What follows is the task.
fn strip_lead_in(text: &str) -> Option<&str> {
    let (lead, rest) = text.split_once(": ")?;
    let words = lead.split_whitespace().count();
    let sentence = lead.contains(['.', '?', '!', '/', '\\']);
    // «cree dos archivos: a.txt, b.txt» opens with the task's own verb: the lead-in is the task.
    let is_task = is_task_verb(lead.split_whitespace().next()?);
    (words > 0 && words <= 6 && !sentence && !is_task && rest.trim().chars().count() >= 3)
        .then_some(rest)
}

/// Whether the word is one of the verbs a title is built from («lista», «crea», «revisa»).
fn is_task_verb(word: &str) -> bool {
    let word = plain(word.trim_matches(|c: char| !c.is_alphanumeric()));
    TITLE_VERBS
        .iter()
        .any(|(forms, _)| forms.iter().any(|form| plain(form) == word))
}

/// `Con el visto bueno del operador, …`: the same envelope, said as an endorsement.
fn strip_endorsement(text: &str) -> Option<&str> {
    let rest = strip_ci(text, "Con el visto bueno del operador")?;
    rest.strip_prefix([',', ':'])
}

/// `Por encargo del operador: …`, `Por encargo de <up to four words>: …`: the envelope a task
/// carries when the operator gave it. Without the colon it is no envelope and stays.
fn strip_commission(text: &str) -> Option<&str> {
    let (first, mut tail) = next_word(strip_ci(text, "Por encargo ")?)?;
    if !first.eq_ignore_ascii_case("de") && !first.eq_ignore_ascii_case("del") {
        return None;
    }
    for _ in 0..4 {
        let (word, rest) = next_word(tail)?;
        if word.ends_with(':') {
            return Some(rest);
        }
        tail = rest;
    }
    None
}

/// The first word of `text` and what follows it.
fn next_word(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    let (word, rest) = text.split_at(end);
    (!word.is_empty()).then_some((word, rest))
}

/// `(a|al|a la) <up to six words> que` after «Pídele», «Dile», «Pídeme» or «Encárgale».
fn strip_order(text: &str) -> Option<&str> {
    const VERBS: [&str; 4] = ["Pídele ", "Pídeme ", "Dile ", "Encárgale "];
    let mut tail = VERBS.iter().find_map(|verb| strip_ci(text, verb))?;
    let mut words: Vec<String> = Vec::new();
    loop {
        let trimmed = tail.trim_start();
        let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        let (word, after) = trimmed.split_at(end);
        if word.is_empty() {
            return None;
        }
        if word.to_lowercase() == "que" {
            return destination_fits(&words).then_some(after);
        }
        words.push(word.to_lowercase());
        tail = after;
    }
}

fn destination_fits(words: &[String]) -> bool {
    let person = match words.first().map(String::as_str) {
        None => return true,
        Some("a") if words.get(1).map(String::as_str) == Some("la") => &words[2..],
        Some("a" | "al") => &words[1..],
        Some(_) => return false,
    };
    person.len() <= 6
}

/// `text` starts with `prefix`, compared without case; the rest of `text` when it does.
fn strip_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let count = prefix.chars().count();
    let (head, rest) = match text.char_indices().nth(count) {
        Some((index, _)) => text.split_at(index),
        None => (text, ""),
    };
    (head.to_lowercase() == prefix.to_lowercase()).then_some(rest)
}

/// The verbs a title can open with: the forms a person writes (infinitive, «crea», «cree»,
/// «creemos») and the infinitive the title shows. A table, not a suffix rule.
const TITLE_VERBS: &[(&[&str], &str)] = &[
    (&["crear", "crea", "cree", "creemos"], "Crear"),
    (&["borrar", "borra", "borre", "borremos"], "Borrar"),
    (
        &["eliminar", "elimina", "elimine", "eliminemos"],
        "Eliminar",
    ),
    (
        &["escribir", "escribe", "escriba", "escribamos"],
        "Escribir",
    ),
    (&["leer", "lee", "lea", "leamos"], "Leer"),
    (&["revisar", "revisa", "revise", "revisemos"], "Revisar"),
    (&["resumir", "resume", "resuma", "resumamos"], "Resumir"),
    (&["buscar", "busca", "busque", "busquemos"], "Buscar"),
    (&["enviar", "envía", "envíe", "enviemos"], "Enviar"),
    (
        &["redactar", "redacta", "redacte", "redactemos"],
        "Redactar",
    ),
    (
        &["preparar", "prepara", "prepare", "preparemos"],
        "Preparar",
    ),
    (&["abrir", "abre", "abra", "abramos"], "Abrir"),
    (&["copiar", "copia", "copie", "copiemos"], "Copiar"),
    (&["mover", "mueve", "mueva", "movamos"], "Mover"),
    (
        &["renombrar", "renombra", "renombre", "renombremos"],
        "Renombrar",
    ),
    (
        &["instalar", "instala", "instale", "instalemos"],
        "Instalar",
    ),
    (
        &["ejecutar", "ejecuta", "ejecute", "ejecutemos"],
        "Ejecutar",
    ),
    (&["probar", "prueba", "pruebe", "probemos"], "Probar"),
    (
        &["arreglar", "arregla", "arregle", "arreglemos"],
        "Arreglar",
    ),
    (
        &["explicar", "explica", "explique", "expliquemos"],
        "Explicar",
    ),
    (&["listar", "lista", "liste", "listemos"], "Listar"),
    (
        &["comparar", "compara", "compare", "comparemos"],
        "Comparar",
    ),
    (
        &["programar", "programa", "programe", "programemos"],
        "Programar",
    ),
    (&["agendar", "agenda", "agende", "agendemos"], "Agendar"),
    (
        &["contratar", "contrata", "contrate", "contratemos"],
        "Contratar",
    ),
    (&["delegar", "delega", "delegue", "deleguemos"], "Delegar"),
    (&["guardar", "guarda", "guarde", "guardemos"], "Guardar"),
    (&["cambiar", "cambia", "cambie", "cambiemos"], "Cambiar"),
    (
        &["descargar", "descarga", "descargue", "descarguemos"],
        "Descargar",
    ),
    (&["subir", "sube", "suba", "subamos"], "Subir"),
    (
        &["traducir", "traduce", "traduzca", "traduzcamos"],
        "Traducir",
    ),
    (
        &["calcular", "calcula", "calcule", "calculemos"],
        "Calcular",
    ),
    (
        &["analizar", "analiza", "analice", "analicemos"],
        "Analizar",
    ),
    (
        &["organizar", "organiza", "organice", "organicemos"],
        "Organizar",
    ),
    (
        &["actualizar", "actualiza", "actualice", "actualicemos"],
        "Actualizar",
    ),
    (
        &["corregir", "corrige", "corrija", "corrijamos"],
        "Corregir",
    ),
    (
        &["cancelar", "cancela", "cancele", "cancelemos"],
        "Cancelar",
    ),
    (
        &["confirmar", "confirma", "confirme", "confirmemos"],
        "Confirmar",
    ),
    (&["avisar", "avisa", "avise", "avisemos"], "Avisar"),
    (&["llamar", "llama", "llame", "llamemos"], "Llamar"),
    (
        &["responder", "responde", "responda", "respondamos"],
        "Responder",
    ),
    (&["pagar", "paga", "pague", "paguemos"], "Pagar"),
    (&["editar", "edita", "edite", "editemos"], "Editar"),
    (&["mostrar", "muestra", "muestre", "mostremos"], "Mostrar"),
    (&["ver", "vea", "veamos"], "Ver"),
    (&["mirar", "mira", "mire", "miremos"], "Mirar"),
];

/// Words that stand before an object without naming it.
const FILLER: &[&str] = &[
    "el", "la", "los", "las", "un", "una", "unos", "unas", "uno", "dos", "tres", "cuatro", "cinco",
    "mi", "mis", "tu", "tus", "su", "sus", "este", "esta", "estos", "estas", "ese", "esa", "esos",
    "esas",
];

/// «Verbo objeto» for a request that opens with a known verb («Cree el archivo redi-dos.txt…»
/// gives «Crear redi-dos.txt»). `None` when the first word is no known verb or nothing names
/// the object: the title then keeps the request's own words.
fn verb_summary(body: &str) -> Option<String> {
    let mut words = body.split_whitespace();
    let first = plain(words.next()?.trim_matches(|c: char| !c.is_alphabetic()));
    let (_, verb) = TITLE_VERBS
        .iter()
        .find(|(forms, _)| forms.iter().any(|form| plain(form) == first))?;
    let object = object_of(&words.collect::<Vec<_>>())?;
    Some(format!("{verb} {object}"))
}

/// What the verb acts on: the file names, else the quoted texts, else up to three content
/// words of the rest of the request, cut at «con», «que» or a mark of punctuation.
fn object_of(rest: &[&str]) -> Option<String> {
    // A name given twice is one file: the first mention keeps its place.
    let mut files: Vec<String> = Vec::new();
    for name in rest.iter().filter_map(|word| file_name(word)) {
        if !files.iter().any(|known| known == name) {
            files.push(name.to_string());
        }
    }
    let items = if files.is_empty() {
        quoted(&rest.join(" "))
    } else {
        files
    };
    if !items.is_empty() {
        let mut text = items
            .iter()
            .take(2)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" y ");
        if items.len() > 2 {
            text.push_str(&format!(" y {} más", items.len() - 2));
        }
        return Some(text);
    }
    let mut words: Vec<&str> = Vec::new();
    let mut index = 0;
    let mut ended = false;
    while index < rest.len() && words.len() < 3 {
        let word = rest[index];
        let bare = word.trim_matches(|c: char| !c.is_alphanumeric());
        let lower = plain(bare);
        // «que» opens a clause; «qué» is a question word and belongs to the object.
        if lower == "con" || bare.to_lowercase() == "que" {
            break;
        }
        // «y revisa …» starts a second task, not more of this object.
        let next_is_verb = rest.get(index + 1).is_some_and(|next| is_task_verb(next));
        if !words.is_empty() && matches!(lower.as_str(), "y" | "e") && next_is_verb {
            break;
        }
        index += 1;
        ended = word.ends_with([':', ',', '.', ';']);
        if !bare.is_empty() && !FILLER.contains(&lower.as_str()) {
            words.push(bare);
        }
        if ended {
            break;
        }
    }
    // A title must not stop after a number or a linking word: it takes the next word
    // (four at most), or drops a linking word that has none.
    if let Some(&last) = words.last().filter(|last| wants_next(last)) {
        // A mark of punctuation closes the sentence: no word follows it to take.
        let next = if ended {
            None
        } else {
            rest.get(index)
                .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()))
                .filter(|bare| !bare.is_empty() && !matches!(plain(bare).as_str(), "con" | "que"))
        };
        match next {
            Some(next) => words.push(next),
            None if is_link(last) => {
                words.pop();
            }
            None => {}
        }
    }
    (!words.is_empty()).then(|| words.join(" "))
}

/// Linking words: a title that ends on one of them is cut mid-sentence.
const LINKS: &[&str] = &[
    "de", "del", "la", "el", "los", "las", "un", "una", "y", "con", "en", "a", "para", "por", "que",
];

fn is_link(word: &str) -> bool {
    LINKS.contains(&plain(word).as_str())
}

/// A number or a linking word, the words after which a title goes on.
fn wants_next(word: &str) -> bool {
    is_link(word) || word.chars().all(|c| c.is_ascii_digit())
}

/// The file a word names: `redi-dos.txt`, `README.md`, or `a.txt` out of `C:\ruta\a.txt`. A
/// name has a dot and an extension of one to five letters or digits, with a letter in it.
fn file_name(word: &str) -> Option<&str> {
    let name = word
        .trim_matches(|c: char| !c.is_alphanumeric())
        .rsplit(['\\', '/'])
        .next()?;
    let (stem, extension) = name.rsplit_once('.')?;
    let extension_ok = (1..=5).contains(&extension.chars().count())
        && extension.chars().all(char::is_alphanumeric)
        && extension.chars().any(char::is_alphabetic);
    (extension_ok && stem.chars().any(char::is_alphanumeric)).then_some(name)
}

/// The texts between «…» or "…", marks included, in order.
fn quoted(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(['«', '"']) {
        let Some(open) = rest[start..].chars().next() else {
            break;
        };
        let close = if open == '«' { '»' } else { '"' };
        let inside = start + open.len_utf8();
        let Some(end) = rest[inside..].find(close) else {
            break;
        };
        let stop = inside + end + close.len_utf8();
        found.push(rest[start..stop].to_string());
        rest = &rest[stop..];
    }
    found
}

/// A word in lower case without accents, so «envía», «envia» and «ENVÍE» compare alike.
fn plain(word: &str) -> String {
    word.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' => 'u',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod title_tests {
    use super::title_from;

    #[test]
    fn the_first_line_becomes_a_short_title() {
        // «creemos» is a known verb now: the title says «Crear proyecto web».
        assert_eq!(
            title_from("creemos un proyecto web").as_deref(),
            Some("Crear proyecto web")
        );
        assert_eq!(
            title_from("Cuáles son los trabajadores que podemos contratar para trabajar en un proyecto de software?").as_deref(),
            Some("Cuáles son los trabajadores que podemos…")
        );
        // No known verb: the first clause is the title, with no «…» (V22).
        assert_eq!(
            title_from("Pídele al líder del proyecto Bench que me diga en qué rama está el proyecto, cuáles son los últimos 5 commits y si hay archivos sin guardar").as_deref(),
            Some("En qué rama está el proyecto")
        );
        assert_eq!(
            title_from("Indica en qué rama está el proyecto, cuáles son los últimos 5 commits").as_deref(),
            Some("Indica en qué rama está el proyecto")
        );
        // The delegation's own wording (V23): the lead-in goes, a clause of 41 characters is cut
        // at a word with no «…», and no linking word is left at its end.
        assert_eq!(
            title_from("Solo lectura, sin cambiar nada: indica en qué rama está el proyecto Bench, los últimos 5 commits (hash corto y mensaje) y si hay archivos sin guardar").as_deref(),
            Some("Indica en qué rama está el proyecto")
        );
        // A comma too early (under 12 characters) is no clause: «Hola, Alfred» stays whole.
        assert_eq!(title_from("Hola, Alfred, ¿cómo estás hoy?").as_deref(), Some("Hola, Alfred, ¿cómo estás hoy"));
        assert_eq!(
            title_from("\n\n**Hola**, Alfred.\nsegunda línea").as_deref(),
            Some("Hola, Alfred")
        );
        assert_eq!(title_from("   \n "), None);
        assert_eq!(
            title_from("¿Qué día es hoy?").as_deref(),
            Some("¿Qué día es hoy")
        );
    }

    #[test]
    fn a_title_with_its_ellipsis_stays_within_forty_characters() {
        let cut = title_from("uno dos tres cuatro cinco seis siete ocho nueve diez once doce")
            .expect("title");
        assert!(cut.chars().count() <= 40, "{cut}");
        assert!(cut.ends_with('…'), "{cut}");
        // Exactly forty characters fit whole, with no ellipsis.
        assert_eq!(
            title_from("abcdefghij abcdefghij abcdefghij abcdefg").as_deref(),
            Some("Abcdefghij abcdefghij abcdefghij abcdefg")
        );
        assert_eq!(
            title_from("abcdefghij abcdefghij abcdefghij abcdefgh").as_deref(),
            Some("Abcdefghij abcdefghij abcdefghij…")
        );
    }

    #[test]
    fn a_known_verb_and_its_object_make_the_title() {
        assert_eq!(
            title_from("Cree el archivo redi-dos.txt con el texto dos").as_deref(),
            Some("Crear redi-dos.txt")
        );
        assert_eq!(
            title_from("Borra el README.md").as_deref(),
            Some("Borrar README.md")
        );
        assert_eq!(
            title_from("Crea uno-t.txt y dos-t.txt").as_deref(),
            Some("Crear uno-t.txt y dos-t.txt")
        );
        assert_eq!(
            title_from("Resume el correo del banco").as_deref(),
            Some("Resumir correo del banco")
        );
        // A Windows path keeps only its file name.
        assert_eq!(
            title_from("Borra C:\\ruta\\a.txt").as_deref(),
            Some("Borrar a.txt")
        );
        // Quoted text is the object when no file is named.
        assert_eq!(
            title_from("Escribe \"hola mundo\" en la nota").as_deref(),
            Some("Escribir \"hola mundo\"")
        );
        // Two names at most; the rest are counted.
        assert_eq!(
            title_from("Crea a-1.txt, b-2.txt y c-3.txt").as_deref(),
            Some("Crear a-1.txt y b-2.txt y 1 más")
        );
        // A name given twice is one file, so the count is of the distinct names.
        assert_eq!(
            title_from("Ejecuta README.md, README.md y b-2.txt").as_deref(),
            Some("Ejecutar README.md y b-2.txt")
        );
        assert_eq!(
            title_from("Ejecuta README.md, README.md, b-2.txt y c-3.txt").as_deref(),
            Some("Ejecutar README.md y b-2.txt y 1 más")
        );
        // «mostrar» and «ver» are verbs too.
        assert_eq!(
            title_from("Ver el correo del banco").as_deref(),
            Some("Ver correo del banco")
        );
        assert_eq!(
            title_from("Muestre el contenido de README.md y después lo borre").as_deref(),
            Some("Mostrar README.md")
        );
        assert_eq!(
            title_from("Pídele al líder del proyecto Bench que mire qué archivos hay en la carpeta del proyecto y el estado de git, sin cambiar nada").as_deref(),
            Some("Mirar qué archivos hay")
        );
        // A short lead-in closed by a colon is no part of the title.
        assert_eq!(
            title_from("Con un solo comando de shell: muestra el contenido de README.md y después bórralo").as_deref(),
            Some("Mostrar README.md")
        );
        assert_eq!(
            title_from("Solo lectura, no cambies nada: lista los archivos de la carpeta").as_deref(),
            Some("Listar archivos de carpeta")
        );
        // A place before the task tells where, not what, so it is no part of the title.
        assert_eq!(
            title_from("En la carpeta del proyecto Bench (bench-proj), ejecuta un único comando de shell que muestre el contenido de README.md y luego lo borre").as_deref(),
            Some("Ejecutar README.md")
        );
        assert_eq!(
            title_from("SOLO LECTURA: en la carpeta del proyecto Bench (bench-proj), lista los archivos y revisa el estado de git").as_deref(),
            Some("Listar archivos")
        );
        // A long sentence before the colon, or a colon with no space after it, stays.
        assert_eq!(title_from("http://x.test/a").as_deref(), Some("Http://x.test/a"));
        // A verb with no object keeps the words of the request.
        assert_eq!(title_from("Crea.").as_deref(), Some("Crea"));
        // No known verb: the title is the request's own first words, as before.
        assert_eq!(
            title_from("Ernesto aparece detenido.").as_deref(),
            Some("Ernesto aparece detenido")
        );
    }

    #[test]
    fn the_request_frame_is_left_out_of_the_title() {
        // The verb and its file names: «Crear uno-t.txt y uno-b.txt» (was the cut sentence).
        assert_eq!(
            title_from("Pídele al líder del proyecto Bench que cree dos archivos: uno-t.txt con el texto uno y uno-b.txt con el texto dos.").as_deref(),
            Some("Crear uno-t.txt y uno-b.txt")
        );
        assert_eq!(
            title_from("Pídele al líder del proyecto Bench que muestre el contenido de README.md y después lo borre, todo en un solo comando de shell").as_deref(),
            Some("Mostrar README.md")
        );
        // Up to three content words after the verb, the article skipped.
        assert_eq!(
            title_from("Crea una lista de tareas para la semana").as_deref(),
            Some("Crear lista de tareas")
        );
        assert_eq!(
            title_from("Por favor, resume este correo").as_deref(),
            Some("Resumir correo")
        );
        assert_eq!(
            title_from("¿Puedes resumir el correo?").as_deref(),
            Some("Resumir correo")
        );
        assert_eq!(
            title_from("Dile lo que quieras").as_deref(),
            Some("Dile lo que quieras")
        );
        assert_eq!(
            title_from("Dile a Ernesto que").as_deref(),
            Some("Dile a Ernesto que")
        );
    }

    #[test]
    fn the_commission_envelope_is_left_out_of_the_title() {
        assert_eq!(
            title_from("Con el visto bueno del operador, borra el archivo README.md del proyecto Bench")
                .as_deref(),
            Some("Borrar README.md")
        );
        assert_eq!(
            title_from("Por encargo del operador: borra el archivo README.md del proyecto Bench")
                .as_deref(),
            Some("Borrar README.md")
        );
        assert_eq!(
            title_from("Por encargo de Ana Pérez: resume el correo del banco").as_deref(),
            Some("Resumir correo del banco")
        );
        // No colon within four words: the envelope is not one, nothing is cut.
        assert!(title_from("Por encargo de la mañana a la tarde sin nada")
            .unwrap()
            .starts_with("Por encargo"));
    }

    #[test]
    fn a_title_does_not_stop_after_a_number_or_a_link() {
        // A number takes the word after it, four words at most.
        assert_eq!(
            title_from("Escribe un párrafo de 200 palabras").as_deref(),
            Some("Escribir párrafo de 200 palabras")
        );
        // Nothing follows the number: it stays.
        assert_eq!(
            title_from("Escribe un párrafo de 200").as_deref(),
            Some("Escribir párrafo de 200")
        );
        // A link with no word after it is dropped.
        assert_eq!(
            title_from("Escribe un correo a").as_deref(),
            Some("Escribir correo")
        );
        // A mark of punctuation closes the sentence: the link goes, nothing is taken across it.
        assert_eq!(
            title_from("Escribe un párrafo de, por favor").as_deref(),
            Some("Escribir párrafo")
        );
    }
}
