import 'package:herdr_notify/src/rust/api/state.dart';

/// "The last revision wins", in one place.
///
/// The gateway publishes a change, not the whole state, so the client re-reads
/// `/v1/state` instead of patching locally; a re-read that races a newer frame
/// must not roll the screen back. Equal revisions are allowed.
class StateReconciler {
  StateSnapshot? _lastGood;

  StateSnapshot? get lastGood => _lastGood;

  int get revision => _lastGood?.revision ?? 0;

  /// Whether a change frame (of [frameRevision], null when absent) is worth a
  /// refresh.
  bool shouldRefreshForFrame(int? frameRevision) {
    if (frameRevision == null) return true;
    if (revision != 0 && frameRevision <= revision) return false;
    return true;
  }

  /// Apply a full read; false when it is older than what is already shown.
  bool acceptState(StateSnapshot next) {
    final current = _lastGood;
    if (current != null && next.revision < current.revision) return false;
    _lastGood = next;
    return true;
  }
}
