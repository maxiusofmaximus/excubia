import 'dart:convert';

/// Retry steps after a lost stream, in seconds. The last step repeats.
const List<int> alertRetrySeconds = [5, 10, 15, 20, 30, 45, 60, 120];

/// Silence longer than this means the stream is dead, even if the socket is open.
const Duration alertHeartbeat = Duration(seconds: 60);

/// Wait before the reconnect that follows [attempt] failed tries; attempt 1 is the first.
Duration alertBackoff(int attempt) {
  final index = (attempt - 1).clamp(0, alertRetrySeconds.length - 1).toInt();
  return Duration(seconds: alertRetrySeconds[index]);
}

/// The `id:` field of one SSE line, or null. Its value comes back as `Last-Event-ID`.
String? sseIdOf(String line) {
  if (!line.startsWith('id:')) return null;
  final value = line.substring(3);
  return value.startsWith(' ') ? value.substring(1) : value;
}

/// What the alerts need from a decoded frame: the chat event name, its status and, for an
/// `alert` frame, the kind the host decided.
class AlertFrame {
  const AlertFrame({required this.event, this.status, this.alert});

  final String event;
  final String? status;
  final AlertKind? alert;
}

/// Turns one line of the stream into a frame, or null when it carries nothing.
typedef AlertDecode = AlertFrame? Function(String line);

/// One decoder per connection: the Rust side keeps the framing state.
typedef AlertDecoderFactory = AlertDecode Function();

enum AlertKind { approval, review, blocked, done, reminder }

/// Approvals always alert. A finished delegation alerts only while the host leaves the
/// decision to the phone (`hostDecides` false). An `alert` frame alerts as the host decided;
/// every other frame only keeps the stream alive.
AlertKind? alertKindOf(AlertFrame frame, {bool hostDecides = false}) =>
    switch (frame.event) {
      'chat.approval' => AlertKind.approval,
      'chat.delegation' when frame.status == 'done' && !hostDecides =>
        AlertKind.review,
      'alert' => frame.alert,
      _ => null,
    };

/// The notification channel of each alert kind (AlertNotifier.kt).
extension AlertKindChannel on AlertKind {
  String get channel => switch (this) {
    AlertKind.approval || AlertKind.blocked => 'agents',
    AlertKind.review || AlertKind.done => 'agents-review',
    AlertKind.reminder => 'recordatorios',
  };
}

/// The kind an `alert` frame names, from its `data:` line. The body holds the kind, a rule id
/// and a time, never an agent name; a kind this app does not know is ignored.
AlertKind? hostAlertKindOf(String dataLine) {
  if (!dataLine.startsWith('data:')) return null;
  final Object? body;
  try {
    body = jsonDecode(dataLine.substring(5));
  } on FormatException {
    return null;
  }
  if (body is! Map<String, dynamic>) return null;
  return switch (body['kind']) {
    'agent_blocked' => AlertKind.blocked,
    'agent_done' => AlertKind.done,
    'calendar_reminder' => AlertKind.reminder,
    _ => null,
  };
}

/// Whether `/v1/state` carries `alert_rules`: a host that has them decides which changes alert.
/// An older host, or a state that is not JSON, leaves the decision to the phone.
bool hostDecidesAlerts(String stateJson) {
  final Object? decoded;
  try {
    decoded = jsonDecode(stateJson);
  } on FormatException {
    return false;
  }
  return decoded is Map && decoded.containsKey('alert_rules');
}
