use chrono::{DateTime, NaiveDate, Weekday};

use super::{End, Freq, RecurrenceError, Rule, Until};

fn parsed(text: &str) -> Rule {
    Rule::parse(text).unwrap_or_else(|error| panic!("{text}: {error}"))
}

fn unsupported(text: &str) -> bool {
    matches!(Rule::parse(text), Err(RecurrenceError::Unsupported(_)))
}

fn invalid(text: &str) -> bool {
    matches!(Rule::parse(text), Err(RecurrenceError::Invalid(_)))
}

#[test]
fn a_weekly_rule_is_normalized_in_a_fixed_order() {
    let rule = parsed("RRULE:count=10;byday=WE,MO;interval=2;freq=weekly");
    assert_eq!(
        rule.to_string(),
        "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;COUNT=10"
    );
    assert_eq!(rule.freq, Freq::Weekly);
    assert_eq!(rule.byday, [Weekday::Mon, Weekday::Wed]);
    assert_eq!(rule.end, End::Count(10));
}

#[test]
fn a_rule_without_interval_or_end_is_explicit_about_both() {
    assert_eq!(parsed("FREQ=DAILY").to_string(), "FREQ=DAILY;INTERVAL=1");
    let rule = parsed("FREQ=MONTHLY");
    assert_eq!(rule.interval, 1);
    assert_eq!(rule.end, End::Forever);
}

#[test]
fn byday_is_sorted_and_without_repeats() {
    let rule = parsed("FREQ=WEEKLY;BYDAY=FR,MO,FR");
    assert_eq!(rule.byday, [Weekday::Mon, Weekday::Fri]);
    assert_eq!(rule.to_string(), "FREQ=WEEKLY;INTERVAL=1;BYDAY=MO,FR");
}

#[test]
fn an_until_is_a_utc_instant_or_a_date() {
    let instant = parsed("FREQ=YEARLY;UNTIL=20301231T235959Z");
    let at = DateTime::parse_from_rfc3339("2030-12-31T23:59:59Z").expect("instant");
    assert_eq!(
        instant.end,
        End::Until(Until::Instant(at.with_timezone(&chrono::Utc)))
    );
    assert_eq!(
        instant.to_string(),
        "FREQ=YEARLY;INTERVAL=1;UNTIL=20301231T235959Z"
    );
    let day = parsed("FREQ=MONTHLY;UNTIL=20301231");
    assert_eq!(
        day.end,
        End::Until(Until::Date(
            NaiveDate::from_ymd_opt(2030, 12, 31).expect("day")
        ))
    );
    assert_eq!(day.to_string(), "FREQ=MONTHLY;INTERVAL=1;UNTIL=20301231");
}

#[test]
fn a_normalized_rule_reads_back_as_the_same_rule() {
    for text in [
        "FREQ=DAILY;INTERVAL=3;COUNT=7",
        "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;COUNT=10",
        "FREQ=WEEKLY;BYDAY=SU",
        "FREQ=MONTHLY;UNTIL=20301231",
        "FREQ=YEARLY;INTERVAL=999;UNTIL=20301231T235959Z",
    ] {
        let rule = parsed(text);
        assert_eq!(parsed(&rule.to_string()), rule, "{text}");
    }
}

#[test]
fn rrule_outside_the_subset_is_unsupported_and_says_which_part() {
    assert!(unsupported("FREQ=MONTHLY;BYMONTHDAY=1"));
    assert!(unsupported("FREQ=DAILY;BYSETPOS=1"));
    assert!(unsupported("FREQ=HOURLY"));
    assert!(unsupported("FREQ=YEARLY;BYMONTH=3"));
    assert!(unsupported("FREQ=MONTHLY;BYDAY=MO"));
    assert!(unsupported("FREQ=WEEKLY;BYDAY=1MO"));
    let Err(RecurrenceError::Unsupported(part)) = Rule::parse("FREQ=DAILY;BYSETPOS=1") else {
        panic!("unsupported part");
    };
    assert_eq!(part, "BYSETPOS");
}

#[test]
fn malformed_or_out_of_range_rules_are_invalid() {
    assert!(invalid(""));
    assert!(invalid("INTERVAL=2"));
    assert!(invalid("FREQ=SOMETIMES"));
    assert!(invalid("FREQ=DAILY;COUNT"));
    assert!(invalid("FREQ=DAILY;FREQ=WEEKLY"));
    assert!(invalid("FREQ=WEEKLY;INTERVAL=0"));
    assert!(invalid("FREQ=WEEKLY;INTERVAL=1000"));
    assert!(invalid("FREQ=DAILY;COUNT=0"));
    assert!(invalid("FREQ=DAILY;COUNT=1001"));
    assert!(invalid("FREQ=WEEKLY;BYDAY=XX"));
    assert!(invalid("FREQ=WEEKLY;BYDAY="));
    assert!(invalid("FREQ=DAILY;UNTIL=20301231T000000"));
    assert!(invalid("FREQ=DAILY;UNTIL=2030-12-31"));
}

#[test]
fn count_and_until_never_come_together() {
    assert!(invalid("FREQ=DAILY;COUNT=3;UNTIL=20301231T000000Z"));
}

#[test]
fn the_rrule_prefix_is_optional_and_spacing_is_forgiven() {
    assert_eq!(
        parsed(" rrule:freq=daily ; count = 2 ").to_string(),
        "FREQ=DAILY;INTERVAL=1;COUNT=2"
    );
}
