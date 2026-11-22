//! The verdicts of the policy: where a pending approval goes next, and why.

/// The level a pending approval is sent to. The store names the levels an
/// approval climbs through; the rules only say where they send it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Lead,
    User,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lead => "lead",
            Self::User => "user",
        }
    }
}

/// Why a pending approval left the policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    NoRule,
    UnknownAction,
    HighRisk,
    NeverDelegate,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoRule => "no_rule",
            Self::UnknownAction => "unknown_action",
            Self::HighRisk => "high_risk",
            Self::NeverDelegate => "never_delegate",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow { rule: String },
    Deny { rule: String },
    Escalate { level: Level, reason: Reason },
}
