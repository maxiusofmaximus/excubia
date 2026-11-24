//! The language of the words the core and the shell put in front of the operator.
//!
//! Spanish is the start value. The shell sets the language once per desktop
//! instance (`set_ui_lang`), and each text is chosen when its message is built.
//! Pure helpers take the language as an argument, so tests check English without
//! touching the process-wide setting.

use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiLang {
    Es,
    En,
}

static ENGLISH: AtomicBool = AtomicBool::new(false);

impl UiLang {
    /// The two codes the shell sends. Anything else is refused.
    pub fn parse(code: &str) -> Option<UiLang> {
        match code {
            "es" => Some(UiLang::Es),
            "en" => Some(UiLang::En),
            _ => None,
        }
    }

    /// The Spanish or the English words, whichever this language is.
    pub fn pick(self, es: &'static str, en: &'static str) -> &'static str {
        match self {
            UiLang::Es => es,
            UiLang::En => en,
        }
    }

    pub fn is_en(self) -> bool {
        self == UiLang::En
    }
}

/// Sets the language of this desktop instance.
pub fn set(lang: UiLang) {
    ENGLISH.store(lang.is_en(), Ordering::Relaxed);
}

/// The language messages are built in right now.
pub fn current() -> UiLang {
    if ENGLISH.load(Ordering::Relaxed) {
        UiLang::En
    } else {
        UiLang::Es
    }
}

/// `current().pick(es, en)`, for a literal that is built right now.
pub fn pick(es: &'static str, en: &'static str) -> &'static str {
    current().pick(es, en)
}

/// A message kept in both languages, so a failure stored once is shown in the language of each send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Localized {
    es: String,
    en: String,
}

impl Localized {
    /// Builds both wordings now. The closure gets the language, so it never reads the global.
    pub fn build(build: impl Fn(UiLang) -> String) -> Self {
        Localized {
            es: build(UiLang::Es),
            en: build(UiLang::En),
        }
    }

    pub fn text(&self, lang: UiLang) -> &str {
        match lang {
            UiLang::Es => &self.es,
            UiLang::En => &self.en,
        }
    }
}

/// Text the caller already built, or external output: both languages show the same words.
impl From<String> for Localized {
    fn from(text: String) -> Self {
        Localized {
            es: text.clone(),
            en: text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_message_is_read_in_the_language_asked_for() {
        let message = Localized::build(|lang| lang.pick("sin núcleo", "no core").to_string());
        assert_eq!(message.text(UiLang::Es), "sin núcleo");
        assert_eq!(message.text(UiLang::En), "no core");
        assert_eq!(Localized::from("x".to_string()).text(UiLang::En), "x");
    }

    #[test]
    fn only_the_two_codes_the_shell_sends_are_accepted() {
        assert_eq!(UiLang::parse("es"), Some(UiLang::Es));
        assert_eq!(UiLang::parse("en"), Some(UiLang::En));
        assert_eq!(UiLang::parse("EN"), None);
        assert_eq!(UiLang::parse("fr"), None);
    }

    #[test]
    fn pick_chooses_by_the_language_it_is_given() {
        assert_eq!(UiLang::Es.pick("hola", "hello"), "hola");
        assert_eq!(UiLang::En.pick("hola", "hello"), "hello");
    }
}
