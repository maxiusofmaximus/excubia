//! The device's side of the pinned transport: the HTTP/1.1 framing and the TLS
//! exchange that `api::pinned` exposes to Dart. Nothing here is public API.

pub(crate) mod framing;
pub(crate) mod transport;

#[cfg(test)]
mod tls_tests;
