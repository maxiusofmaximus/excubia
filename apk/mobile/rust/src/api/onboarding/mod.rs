//! The first-run team: the role catalogue, the onboarding answer and the hires as
//! the host sends them. Routes and bodies are built in `plan`, answers are read
//! in `model`, refusals are explained in `error`. Dart only signs and sends.

pub mod error;
pub mod model;
pub mod plan;

#[cfg(test)]
mod tests;
