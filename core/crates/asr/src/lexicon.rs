//! Project dictionary: the speech engine hears a few project names wrong, so their known
//! mishearings are written correctly in the final text. Whole words only.

/// Heard form (lowercase) and the form that is written.
const TERMS: &[(&str, &str)] = &[
    ("escuya", "Excubia"),
    ("exclubia", "Excubia"),
    ("excubía", "Excubia"),
    ("excubia", "Excubia"),
];

/// Replaces each heard word in `text` with its written form; punctuation and spacing stay.
pub(crate) fn fix_terms(text: &str) -> String {
    let mut fixed = String::with_capacity(text.len());
    let mut word = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            word.push(ch);
            continue;
        }
        push_word(&mut fixed, &word);
        word.clear();
        fixed.push(ch);
    }
    push_word(&mut fixed, &word);
    fixed
}

fn push_word(fixed: &mut String, word: &str) {
    let lower = word.to_lowercase();
    match TERMS.iter().find(|(heard, _)| *heard == lower) {
        Some((_, written)) => fixed.push_str(written),
        None => fixed.push_str(word),
    }
}

#[cfg(test)]
mod tests {
    use super::fix_terms;

    #[test]
    fn fixes_the_misheard_name_in_a_sentence() {
        assert_eq!(
            fix_terms("Quiero que escuya me avise"),
            "Quiero que Excubia me avise"
        );
        assert_eq!(fix_terms("escuya, por favor"), "Excubia, por favor");
        assert_eq!(fix_terms("escuya me avisa"), "Excubia me avisa");
    }

    #[test]
    fn matches_whole_words_ignoring_case() {
        assert_eq!(fix_terms("EXCLUBIA."), "Excubia.");
        assert_eq!(fix_terms("Escuya me avisa"), "Excubia me avisa");
        assert_eq!(fix_terms("excubía y excubia"), "Excubia y Excubia");
    }

    #[test]
    fn leaves_the_real_verb_excluir_alone() {
        // "excluye" and "excluya" are forms of the verb "excluir": "esto excluye aquello" is not a name.
        assert_eq!(fix_terms("esto excluye aquello"), "esto excluye aquello");
        assert_eq!(
            fix_terms("Quiero que excluya, por favor"),
            "Quiero que excluya, por favor"
        );
        assert_eq!(fix_terms("EXCLUYE."), "EXCLUYE.");
    }

    #[test]
    fn leaves_words_that_only_contain_a_term() {
        assert_eq!(fix_terms("un plan excluyente"), "un plan excluyente");
        assert_eq!(fix_terms("excluye2"), "excluye2");
    }

    #[test]
    fn empty_text_stays_empty() {
        assert_eq!(fix_terms(""), "");
    }
}
