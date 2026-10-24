//! The levels an approval waits at, in the order it climbs them (RFC §2, CORE9-C,
//! CORE9-F): `policy` → `scorer` → `lead` → `cto` → `user`. The name in a row is
//! only read through [`ApprovalLevel::from_column`], so a misspelt level is not
//! a level.

use crate::policy::Level;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ApprovalLevel {
    Policy,
    Scorer,
    Lead,
    Cto,
    User,
}

impl ApprovalLevel {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Policy => "policy",
            Self::Scorer => "scorer",
            Self::Lead => "lead",
            Self::Cto => "cto",
            Self::User => "user",
        }
    }

    pub(super) fn from_column(name: &str) -> Option<Self> {
        match name {
            "policy" => Some(Self::Policy),
            "scorer" => Some(Self::Scorer),
            "lead" => Some(Self::Lead),
            "cto" => Some(Self::Cto),
            "user" => Some(Self::User),
            _ => None,
        }
    }

    /// The level above this one; `user` is the top of the ladder.
    pub(super) fn next(self) -> Option<Self> {
        match self {
            Self::Policy | Self::Scorer => Some(Self::Lead),
            Self::Lead => Some(Self::Cto),
            Self::Cto => Some(Self::User),
            Self::User => None,
        }
    }
}

impl From<Level> for ApprovalLevel {
    fn from(level: Level) -> Self {
        match level {
            Level::Lead => Self::Lead,
            Level::User => Self::User,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ApprovalLevel;

    #[test]
    fn the_ladder_climbs_one_level_at_a_time_to_the_operator() {
        assert_eq!(ApprovalLevel::Policy.next(), Some(ApprovalLevel::Lead));
        assert_eq!(ApprovalLevel::Scorer.next(), Some(ApprovalLevel::Lead));
        assert_eq!(ApprovalLevel::Lead.next(), Some(ApprovalLevel::Cto));
        assert_eq!(ApprovalLevel::Cto.next(), Some(ApprovalLevel::User));
        assert_eq!(ApprovalLevel::User.next(), None);
    }

    #[test]
    fn a_stored_name_round_trips_and_an_unknown_one_is_no_level() {
        for level in [
            ApprovalLevel::Policy,
            ApprovalLevel::Scorer,
            ApprovalLevel::Lead,
            ApprovalLevel::Cto,
            ApprovalLevel::User,
        ] {
            assert_eq!(ApprovalLevel::from_column(level.as_str()), Some(level));
        }
        assert_eq!(ApprovalLevel::from_column("leader"), None);
    }
}
