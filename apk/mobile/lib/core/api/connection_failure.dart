import 'package:herdr_notify/src/rust/api/pinned.dart' as rust;

/// Why the PC could not be reached. A [HostError] carries it only when the
/// request failed before an answer; an HTTP status is classified from its code.
enum ConnectionFailure {
  /// No transport completed the handshake with the paired identity.
  noRoute,

  /// The host answered, but its service is down (502, 503 or 504).
  hostDown,

  /// The deadline ran out before an answer.
  timeout,

  /// The host presents a key or certificate other than the paired one.
  identityMismatch,
}

/// Null for the pinned failures that are not about reaching the PC, such as an
/// expired link. A broken handshake is `noRoute`: the identity was never proven.
ConnectionFailure? connectionFailureOfPinned(rust.PinnedFailureKind kind) =>
    switch (kind) {
      rust.PinnedFailureKind.pinMismatch ||
      rust.PinnedFailureKind.certificateInvalid =>
        ConnectionFailure.identityMismatch,
      rust.PinnedFailureKind.timeout => ConnectionFailure.timeout,
      rust.PinnedFailureKind.unreachable ||
      rust.PinnedFailureKind.protocol => ConnectionFailure.noRoute,
      _ => null,
    };
