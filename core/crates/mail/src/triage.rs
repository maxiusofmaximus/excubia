//! The verdict a received mail gets from its checks (F0 of the mail decisions RFC):
//! fixed rules over `Checks`, with no model and no network. The first rule that
//! matches decides the level; `signals` lists every signal the mail has.

use serde::Serialize;

use crate::received::Checks;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    PhishCandidate,
    Suspicious,
    Bulk,
    /// The automatic checks found nothing; never that the mail is safe.
    Clean,
    /// Nothing can be checked: the mail names no authentication result.
    Unknown,
}

impl Level {
    /// The name the level has in the wire format and in the approval payload.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PhishCandidate => "phish_candidate",
            Self::Suspicious => "suspicious",
            Self::Bulk => "bulk",
            Self::Clean => "clean",
            Self::Unknown => "unknown",
        }
    }
}

/// The facts behind a level, in the order they are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    AuthAllFail,
    DmarcFail,
    SpfFail,
    DkimFail,
    AuthMissing,
    ReplyToDiffers,
    ReturnPathDiffers,
    ClaimsAttachmentButNone,
    LinkHostDiffers,
    BulkList,
    OneClickUnsubscribe,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Verdict {
    pub level: Level,
    pub signals: Vec<Signal>,
}

pub fn verdict(checks: &Checks) -> Verdict {
    Verdict {
        level: level(checks),
        signals: signals(checks),
    }
}

fn level(checks: &Checks) -> Level {
    let auth = &checks.auth;
    let words = [auth.spf, auth.dkim, auth.dmarc];
    let any_failed = words.iter().any(|word| failed(word));
    let all_failed = words.iter().all(|word| failed(word));
    let all_unchecked = words.iter().all(|word| unchecked(word));
    let all_passed = words.iter().all(|word| *word == "pass");
    let mismatch = checks.reply_to_differs
        || checks.return_path_differs
        || checks.link_host_differs
        || checks.claims_attachment_but_none;
    let shifted = checks.reply_to_differs || checks.return_path_differs;
    // A bulk mail that passes every check: the sending platform rewrites links and senders.
    let trusted_bulk = checks.bulk && all_passed;

    if all_failed || (auth.dmarc == "fail" && mismatch) {
        Level::PhishCandidate
    } else if any_failed || checks.claims_attachment_but_none {
        Level::Suspicious
    } else if trusted_bulk {
        Level::Bulk
    } else if checks.link_host_differs || shifted {
        Level::Suspicious
    } else if checks.bulk {
        Level::Bulk
    } else if all_unchecked {
        Level::Unknown
    } else {
        Level::Clean
    }
}

fn signals(checks: &Checks) -> Vec<Signal> {
    let auth = &checks.auth;
    let words = [auth.spf, auth.dkim, auth.dmarc];
    [
        (words.iter().all(|word| failed(word)), Signal::AuthAllFail),
        (auth.dmarc == "fail", Signal::DmarcFail),
        (failed(auth.spf), Signal::SpfFail),
        (failed(auth.dkim), Signal::DkimFail),
        (
            words.iter().all(|word| unchecked(word)),
            Signal::AuthMissing,
        ),
        (checks.reply_to_differs, Signal::ReplyToDiffers),
        (checks.return_path_differs, Signal::ReturnPathDiffers),
        (
            checks.claims_attachment_but_none,
            Signal::ClaimsAttachmentButNone,
        ),
        (checks.link_host_differs, Signal::LinkHostDiffers),
        (checks.bulk, Signal::BulkList),
        (checks.one_click_unsubscribe, Signal::OneClickUnsubscribe),
    ]
    .into_iter()
    .filter_map(|(present, signal)| present.then_some(signal))
    .collect()
}

fn failed(word: &str) -> bool {
    matches!(word, "fail" | "softfail")
}

/// The method names no result, so there is nothing to check.
fn unchecked(word: &str) -> bool {
    matches!(word, "missing" | "none")
}
