//! Approvals as the chat shows them: the record model, the one-view reducer, the
//! request plans and the refusal meaning. Dart signs and sends; nothing here
//! touches the network.

pub mod error;
pub mod model;
pub mod plan;
pub mod reducer;

#[cfg(test)]
mod tests;
