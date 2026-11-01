//! The hires (ONBOARD-T1): contracted professionals. A hire is a registered
//! role with a name and an avatar, and it has no pane until its first
//! assignment. It lives apart from `agent_seat`, which keeps the structural
//! roles (alfred, lead, specialist) that delegation and the prompts depend on.

/// The active hires one area may hold at the same time.
pub const MAX_HIRES_PER_AREA: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hire {
    pub id: String,
    pub catalog_role: String,
    pub area: String,
    pub name: String,
    /// The id of the character, never the name: renaming keeps the avatar.
    pub avatar: Option<String>,
    pub engine: Option<String>,
    pub profile: Option<String>,
    pub skills_allow: Vec<String>,
    pub status: HireStatus,
    pub created_at: String,
    pub updated_at: String,
    pub removed_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewHire {
    pub catalog_role: String,
    pub area: String,
    pub name: String,
    pub avatar: Option<String>,
    pub engine: Option<String>,
    pub profile: Option<String>,
    pub skills_allow: Vec<String>,
}

/// What contracting one role did: a new hire, or the active hire the role
/// already had (which is returned unchanged).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HireOutcome {
    Created(Hire),
    Existing(Hire),
}

impl HireOutcome {
    pub fn into_hire(self) -> Hire {
        match self {
            Self::Created(hire) | Self::Existing(hire) => hire,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HireStatus {
    Hired,
    Removed,
}

impl HireStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hired => "hired",
            Self::Removed => "removed",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "hired" => Some(Self::Hired),
            "removed" => Some(Self::Removed),
            _ => None,
        }
    }
}

/// What a listing returns: the hired ones, or the dismissed ones as well.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HireFilter {
    Active,
    All,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnboardingState {
    pub done: bool,
    pub areas: Vec<String>,
}
