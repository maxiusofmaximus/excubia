//! The rules of the host policy (level 1). The first rule that matches
//! decides, the file order is the priority, and a category in
//! `never_delegate` never reaches a rule: it goes to the user.

mod policy;
mod rule;
#[cfg(test)]
mod tests;
mod verdict;

pub use policy::Policy;
pub use rule::{Effect, Facts, Rule, Scope};
pub use verdict::{Level, Reason, Verdict};
