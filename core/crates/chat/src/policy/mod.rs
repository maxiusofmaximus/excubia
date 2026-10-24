//! The host policy (level 1 of `docs/rfc/approvals-escalation.md`): the rules
//! that decide an approval at creation, with no wait and no TTL.
//!
//! The host classifies the action from its structured payload, then the rules
//! of `approvals.toml` (or the defaults) give a verdict: allow, deny, or send
//! the approval on to a level that decides it.

mod category;
mod classify;
mod file;
mod rules;

pub(crate) use category::Category;
pub(crate) use classify::{assess_risk, classify, in_workspace, locate};
pub(crate) use file::PolicySource;
pub(crate) use rules::{Facts, Level, Reason, Verdict};
