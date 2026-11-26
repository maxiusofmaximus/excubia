//! The free OpenCode models Alfred's turns may run on, in the order they are tried, and the
//! ones whose free tier is spent. A model that answered «Free usage limit reached» is skipped
//! for an hour, so the next turns do not pay a failed run to learn it again. Paid models never
//! appear here.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// The free models in order of preference. Only `opencode/*-free` ids belong here.
pub const FREE_MODELS: &[&str] = &[
    "opencode/deepseek-v4.1-flash-free",
    "opencode/nemotron-3.5-lightning-free",
];

/// How long a model whose free tier ran out stays skipped.
pub const EXHAUSTED_FOR: Duration = Duration::from_secs(60 * 60);

/// The failure code of a turn when every free model is spent.
pub const EXHAUSTED_REASON: &str = "free_models_exhausted";

#[derive(Default)]
pub struct FreeModels {
    exhausted: Mutex<HashMap<&'static str, Instant>>,
}

impl FreeModels {
    /// The free models not known to be spent at `now`, in order of preference.
    pub fn usable(&self, now: Instant) -> Vec<&'static str> {
        let exhausted = self.exhausted.lock().unwrap_or_else(PoisonError::into_inner);
        FREE_MODELS
            .iter()
            .copied()
            .filter(|model| match exhausted.get(model) {
                Some(since) => now.duration_since(*since) >= EXHAUSTED_FOR,
                None => true,
            })
            .collect()
    }

    /// Remembers `model` as spent. Only a model of `FREE_MODELS` is kept; any other id is ignored.
    pub fn mark_exhausted(&self, model: &str, now: Instant) {
        let Some(&model) = FREE_MODELS.iter().find(|free| **free == model) else {
            return;
        };
        self.exhausted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(model, now);
    }
}

/// Whether the CLI's output says the free tier of the model is spent. OpenCode prints the
/// `provider.free-limit` code; the sentence is the same text in plain form.
pub fn is_free_limit(output: &str) -> bool {
    let lower = output.to_lowercase();
    lower.contains("provider.free-limit") || lower.contains("free usage limit reached")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_free_model_is_tried_first_and_only_free_ids_are_listed() {
        let book = FreeModels::default();
        assert_eq!(book.usable(Instant::now()), FREE_MODELS.to_vec());
        assert!(FREE_MODELS.iter().all(|model| model.starts_with("opencode/") && model.ends_with("-free")));
        assert_eq!(FREE_MODELS[0], "opencode/deepseek-v4.1-flash-free");
    }

    #[test]
    fn a_spent_model_is_skipped_for_an_hour_then_tried_again() {
        let book = FreeModels::default();
        let now = Instant::now();
        book.mark_exhausted(FREE_MODELS[0], now);
        assert_eq!(book.usable(now + Duration::from_secs(59 * 60)), FREE_MODELS[1..].to_vec());
        assert_eq!(book.usable(now + EXHAUSTED_FOR), FREE_MODELS.to_vec());
    }

    #[test]
    fn every_model_spent_leaves_nothing_to_try() {
        let book = FreeModels::default();
        let now = Instant::now();
        for model in FREE_MODELS {
            book.mark_exhausted(model, now);
        }
        assert!(book.usable(now).is_empty());
    }

    #[test]
    fn the_free_limit_is_seen_in_opencode_and_plain_text_only() {
        assert!(is_free_limit("ProviderError: provider.free-limit"));
        assert!(is_free_limit("Error: Free usage limit reached. Try again later."));
        assert!(!is_free_limit("Claude AI usage limit reached|1730000000"));
        assert!(!is_free_limit("process_exit: 1"));
    }
}
