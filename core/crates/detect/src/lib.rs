//! Detection for the herdr-notify core: which agent is running, and what
//! state its screen is in (`idle` / `working` / `blocked` / `unknown`).
//!
//! The rule engine and the agent alias table are adapted from herdr
//! (Apache-2.0); see NOTICE.

mod identify;
mod manifest;
mod title;

pub use identify::{identify_program, AgentKind};
pub use manifest::{DetectedState, Detection, Detector};
pub use title::TitleTracker;
