/// What a screen reader is told about the step of a turn. A step changes often,
/// so a new one is spoken at most once per [every]; the last one spoken stands
/// until then, and a finished turn speaks nothing.
class StepAnnouncer {
  StepAnnouncer({this.every = const Duration(seconds: 5)});

  final Duration every;
  String? _spoken;
  DateTime? _at;

  /// The live label now: [line] when it may be spoken, else the last one spoken.
  String? label(String? line, DateTime now) {
    if (line == null) {
      _spoken = null;
      return null;
    }
    final last = _at;
    if (line != _spoken && (last == null || now.difference(last) >= every)) {
      _spoken = line;
      _at = now;
    }
    return _spoken;
  }
}
