//! The five statuses of `docs/contracts/state.md`.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Idle,
    Working,
    Blocked,
    Done,
    Unknown,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Idle => "idle",
            Status::Working => "working",
            Status::Blocked => "blocked",
            Status::Done => "done",
            Status::Unknown => "unknown",
        }
    }

    /// Contract ranking: blocked ▸ done ▸ working/unknown ▸ idle.
    pub fn attention(self) -> u8 {
        match self {
            Status::Blocked => 3,
            Status::Done => 2,
            Status::Working | Status::Unknown => 1,
            Status::Idle => 0,
        }
    }
}
