use std::collections::HashSet;
use std::time::{Duration, Instant};

use super::{correct, correct_with, is_common, plain, sound_of, Corrector, THRESHOLD};
use crate::corrector_data::{COMMON_WORDS, LEXICON};

/// Lexicon entries that are also common words. They are kept on purpose: the guard never changes a
/// common word, and an exact match maps to itself. All of them come from the script.
const JUSTIFIED_OVERLAP: &[&str] = &[
    "borra",
    "apruébalo",
    "deniégalo",
    "delega",
    "anota",
    "recuérdame",
    "avísame",
    "escríbele",
    "mueve",
    "crea",
    "prepara",
    "envía",
    "cancela",
    "guarda",
    "busca",
    "para",
    "excubia",
    "alfred",
    "borrador",
    "presupuesto",
    "contacto",
    "nota",
    "aprobación",
    "equipo",
    "cliente",
    "proveedor",
    "proyecto",
];

#[test]
fn sounds_follow_the_spanish_rules() {
    assert_eq!(sound_of("vaca"), "baka");
    assert_eq!(sound_of("hielo"), "ielo");
    assert_eq!(sound_of("llave"), "yabe");
    assert_eq!(sound_of("guerra"), "jerra");
    assert_eq!(sound_of("cielo"), "sielo");
    assert_eq!(sound_of("quiero"), "kiero");
    assert_eq!(sound_of("Excubia"), "eskubia");
}

#[test]
fn lexicon_is_the_measured_list() {
    assert_eq!(LEXICON.len(), 37);
}

#[test]
fn common_words_are_not_changed() {
    assert_eq!(correct("borré la repetida"), "borré la repetida");
    assert_eq!(correct("para nota"), "para nota");
    assert_eq!(correct("de la"), "de la");
    assert_eq!(correct("contactos"), "contactos");
    assert_eq!(correct("borrador"), "borrador");
}

#[test]
fn a_sound_close_to_a_term_changes_to_it() {
    assert_eq!(correct_with("scubia", 0.15), "excubia");
    assert_eq!(correct_with("apruebalo", THRESHOLD), "apruébalo");
    assert_eq!(correct_with("deniegalo", THRESHOLD), "deniégalo");
    assert_eq!(correct_with("envia", THRESHOLD), "envía");
    assert_eq!(correct_with("ex cubia", THRESHOLD), "excubia");
}

#[test]
fn a_strict_threshold_leaves_distant_words_alone() {
    assert_eq!(correct_with("scubia", 0.10), "scubia");
    assert_eq!(
        correct_with("mil doscientos cincuenta", 0.35),
        "mil doscientos cincuenta"
    );
}

#[test]
fn heard_misspellings_are_corrected_in_a_sentence() {
    assert_eq!(correct("Aprovevalo"), "Apruébalo");
    assert_eq!(correct("Scubia"), "Excubia");
    assert_eq!(correct("Escribele"), "Escríbele");
}

#[test]
fn known_misses_stay_as_measured() {
    // shortcut: "Dele da" is blocked by the common "da", and "borral" is 3 letters from "borrador"
    // (0.375 > 0.25). Upgrade when the lexicon or the guard is measured again.
    assert_eq!(correct("Dele da esto al equipo"), "Dele da esto al equipo");
    assert_eq!(correct("Prepara un borral"), "Prepara un borra");
}

#[test]
fn punctuation_and_capitals_survive() {
    assert_eq!(correct("Sí, borra la repetida."), "Sí, borra la repetida.");
    assert_eq!(
        correct("¿Scubia? borra la nota."),
        "¿Excubia? borra la nota."
    );
}

#[test]
fn applying_it_twice_is_applying_it_once() {
    for text in [
        "Aprovevalo ya",
        "Scubia, escribele",
        "Prepara un borral",
        "Dele da esto al equipo",
    ] {
        let once = correct(text);
        assert_eq!(correct(&once), once, "{text}");
    }
}

#[test]
fn empty_text_and_only_signs_do_not_fail() {
    assert_eq!(correct(""), "");
    assert_eq!(correct("   "), "   ");
    assert_eq!(correct("¿?!..."), "¿?!...");
}

#[test]
fn off_returns_the_text_untouched() {
    assert_eq!(Corrector::Off.apply("Scubia"), "Scubia");
    assert_eq!(Corrector::On.apply("Scubia"), "Excubia");
    assert_eq!(Corrector::default(), Corrector::On);
}

#[test]
fn a_twenty_word_sentence_is_fast_enough() {
    // Release builds take a few milliseconds for this loop; the bound only catches a regression.
    let sentence = "quiero que me recuerdes llamar al cliente antes de la reunión del martes \
                    para revisar el presupuesto y el calendario del proyecto nuevo";
    let started = Instant::now();
    for _ in 0..1000 {
        std::hint::black_box(correct(std::hint::black_box(sentence)));
    }
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn common_words_are_unique_and_enough() {
    let mut seen = HashSet::new();
    for word in COMMON_WORDS.split_whitespace() {
        assert!(seen.insert(plain(word)), "duplicate common word: {word}");
    }
    assert!(seen.len() >= 300, "only {} common words", seen.len());
}

#[test]
fn lexicon_has_no_duplicates() {
    let unique: HashSet<&str> = LEXICON.iter().copied().collect();
    assert_eq!(unique.len(), LEXICON.len());
}

#[test]
fn only_justified_lexicon_entries_are_common_words() {
    let overlap: HashSet<&str> = LEXICON
        .iter()
        .copied()
        .filter(|term| is_common(term))
        .collect();
    let justified: HashSet<&str> = JUSTIFIED_OVERLAP.iter().copied().collect();
    assert_eq!(justified.len(), JUSTIFIED_OVERLAP.len());
    assert_eq!(overlap, justified);
}
