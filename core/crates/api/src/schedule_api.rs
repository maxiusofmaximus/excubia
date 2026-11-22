//! The scheduled work over REST (F4-CORE): the body a device sends. The store checks what may be
//! kept (the steps, the plan, the text, the brakes); this module checks the shape of the body and
//! turns the clock fields into a timing, so no bad time reaches the store.

use excubia_chat::{timing_of, ChatError, Content, NewSchedule};
use serde::Deserialize;

/// A schedule a device keeps. Exactly one of `body` and `recipe_id` is given. `time` is the
/// local `HH:MM` in `timezone`; `rule` is an RFC 5545 rule, and absent or null is a one-time run.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleBody {
    pub title: String,
    pub conversation_id: String,
    pub role: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub recipe_id: Option<String>,
    pub start_date: String,
    pub time: String,
    pub timezone: String,
    #[serde(default)]
    pub rule: Option<String>,
    #[serde(default = "one_step")]
    pub steps: u32,
    #[serde(default)]
    pub plan_id: Option<String>,
}

fn one_step() -> u32 {
    1
}

impl ScheduleBody {
    /// The store's form of the body. A body with both texts, or neither, is refused here.
    pub fn new_schedule(&self) -> Result<NewSchedule, ChatError> {
        let content = match (&self.body, &self.recipe_id) {
            (Some(text), None) => Content::Body(text.clone()),
            (None, Some(recipe_id)) => Content::Recipe(recipe_id.clone()),
            _ => return Err(ChatError::InvalidSchedule("body")),
        };
        Ok(NewSchedule {
            title: self.title.clone(),
            conversation_id: self.conversation_id.clone(),
            role_id: self.role.clone(),
            content,
            timing: timing_of(
                &self.start_date,
                &self.time,
                &self.timezone,
                self.rule.as_deref(),
            )?,
            steps: self.steps,
            plan_id: self.plan_id.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(extra: &str) -> ScheduleBody {
        let text = format!(
            r#"{{"title":"Resumen","conversation_id":"conv_1","role":"audio-editor",
                "start_date":"2026-11-02","time":"09:00","timezone":"Europe/Madrid"{extra}}}"#
        );
        serde_json::from_str(&text).expect("a body")
    }

    #[test]
    fn a_text_body_becomes_a_daily_or_one_time_schedule() {
        let daily = body(r#","body":"Haz un resumen.","rule":"FREQ=DAILY;INTERVAL=1""#);
        let new = daily.new_schedule().expect("valid");
        assert_eq!(new.content, Content::Body("Haz un resumen.".into()));
        assert!(matches!(
            new.timing.cadence,
            excubia_chat::Cadence::Repeat(_)
        ));
        assert_eq!(new.steps, 1);

        let once = body(r#","body":"Una vez.""#).new_schedule().expect("valid");
        assert_eq!(once.timing.cadence, excubia_chat::Cadence::Once);
    }

    #[test]
    fn a_body_with_both_texts_or_neither_is_refused() {
        let both = body(r#","body":"x","recipe_id":"rcp_1""#);
        assert!(matches!(
            both.new_schedule(),
            Err(ChatError::InvalidSchedule("body"))
        ));
        let neither = body("");
        assert!(matches!(
            neither.new_schedule(),
            Err(ChatError::InvalidSchedule("body"))
        ));
    }

    #[test]
    fn a_bad_clock_or_zone_is_refused_before_the_store() {
        let bad_time: ScheduleBody = serde_json::from_str(
            r#"{"title":"t","conversation_id":"c","role":"r","body":"x",
                "start_date":"2026-11-02","time":"25:00","timezone":"Europe/Madrid"}"#,
        )
        .expect("a body");
        assert!(matches!(
            bad_time.new_schedule(),
            Err(ChatError::InvalidSchedule("time"))
        ));
        let bad_zone: ScheduleBody = serde_json::from_str(
            r#"{"title":"t","conversation_id":"c","role":"r","body":"x",
                "start_date":"2026-11-02","time":"09:00","timezone":"Mars/Olympus"}"#,
        )
        .expect("a body");
        assert!(matches!(
            bad_zone.new_schedule(),
            Err(ChatError::InvalidSchedule("time"))
        ));
    }

    #[test]
    fn an_unknown_field_in_a_schedule_is_refused() {
        let parsed: Result<ScheduleBody, _> = serde_json::from_str(
            r#"{"title":"t","conversation_id":"c","role":"r","body":"x",
                "start_date":"2026-11-02","time":"09:00","timezone":"Europe/Madrid","owner":"x"}"#,
        );
        assert!(parsed.is_err());
    }
}
