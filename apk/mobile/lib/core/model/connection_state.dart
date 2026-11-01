import 'package:herdr_notify/src/rust/api/error.dart';
import 'package:herdr_notify/src/rust/api/state.dart';

import '../api/connection_failure.dart';

/// The states the app itself can be in.
sealed class ConnectionState {
  const ConnectionState();
}

/// Paired, first request in flight.
class Connecting extends ConnectionState {
  const Connecting();
}

/// [updatedAt] is when the last frame that changed [state] arrived.
class Live extends ConnectionState {
  const Live(this.state, {required this.updatedAt});

  final StateSnapshot state;
  final DateTime updatedAt;
}

/// Paired, was live, lost the host. Keeps showing the last known state.
class Offline extends ConnectionState {
  const Offline(
    this.lastKnown, {
    required this.reason,
    required this.updatedAt,
    this.failure,
  });

  final StateSnapshot? lastKnown;
  final String? reason;
  final DateTime? updatedAt;

  /// Why the PC cannot be reached, when the failure says so.
  final ConnectionFailure? failure;
}

/// Paired, reachable, but something is wrong. [kind] says what fixes it.
class Failed extends ConnectionState {
  const Failed(this.message, {this.action, required this.kind});

  final String message;
  final String? action;
  final FailureKind kind;

  bool get recoverable => kind == FailureKind.retry;
}

/// Paired and reachable, but the host refuses the reads: this device has no
/// `read_state` grant yet. Someone must approve it on the PC. The app waits
/// for a manual retry and never polls for it.
class AwaitingApproval extends ConnectionState {
  const AwaitingApproval();
}
