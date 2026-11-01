import 'dart:convert';

import '../api/json_read.dart';
import '../app_info.dart';

/// The local alert built from `GET /v1/state`. Only counts go in it, so the
/// notification carries no agent text, commands or pane ids.
class AlertNotice {
  const AlertNotice({required this.title, required this.body});

  static const String _title = appName;

  static const AlertNotice generic = AlertNotice(
    title: _title,
    body: 'Hay novedades de tus agentes.',
  );

  /// A notice the host decided on: [body] is its generic text, never an agent's name.
  factory AlertNotice.alert(String body) =>
      AlertNotice(title: _title, body: body);

  final String title;
  final String body;

  /// Approvals waiting for this user come first: they need an action.
  factory AlertNotice.fromState(String json) {
    final Object? decoded;
    try {
      decoded = jsonDecode(json);
    } on FormatException {
      return generic;
    }
    if (decoded is! Map<String, dynamic>) return generic;
    final chat = jsonObjectOrEmpty(decoded['chat']);
    final counts = jsonObjectOrEmpty(decoded['counts']);
    final approvals = _count(chat['pending_user_approvals']);
    if (approvals > 0) {
      return AlertNotice(
        title: _title,
        body: approvals == 1
            ? 'Una aprobación espera tu decisión.'
            : '$approvals aprobaciones esperan tu decisión.',
      );
    }
    final blocked = _count(counts['blocked']);
    if (blocked > 0) {
      return AlertNotice(
        title: _title,
        body: blocked == 1
            ? 'Un agente está bloqueado.'
            : '$blocked agentes están bloqueados.',
      );
    }
    return generic;
  }

  static int _count(Object? value) => value is int ? value : 0;
}
