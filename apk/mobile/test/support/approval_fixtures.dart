import 'dart:convert';

/// The host's approval answers, shaped as `/v1/chat/approvals` returns them.
/// Deadlines sit in the future so the cards open as pending.
String soonIso([Duration ahead = const Duration(minutes: 5)]) =>
    DateTime.now().toUtc().add(ahead).toIso8601String();

Map<String, Object?> approvalRow({
  String id = 'apr_1',
  String status = 'pending',
  String level = 'user',
  String? expiresAt,
  String? question,
  String command = 'cargo test',
  String? conversationId = 'cnv_1',
}) => {
  'id': id,
  'conversation_id': conversationId,
  'seat_id': 'seat_bob',
  'pane_id': 'w1:p9',
  'kind': 'command',
  'tool': 'bash',
  'summary': 'Ejecutar comando: $command',
  'cwd': 'demo/api',
  'risk': 'medium',
  'status': status,
  'level': level,
  'expires_at': expiresAt ?? soonIso(),
  'payload_redacted': {'command': command, 'question': ?question},
};

String inboxJson([List<Map<String, Object?>>? rows]) => jsonEncode({
  'ok': true,
  'approvals': rows ?? [approvalRow()],
});

String recordJson(Map<String, Object?> row) =>
    jsonEncode({'ok': true, 'approval': row});

String stateJson(String status, {String? expiresAt}) => jsonEncode({
  'ok': true,
  'approval_id': 'apr_1',
  'status': status,
  'expires_at': expiresAt ?? soonIso(),
});

String settingsJson({
  String global = 'delegated',
  List<Map<String, Object?>> projects = const [],
}) => jsonEncode({
  'ok': true,
  'settings': {'global': global, 'projects': projects},
  'replayed': false,
  'escalated': 0,
});

const String forbiddenBody = '{"detail":"forbidden"}';
const String grantCommand = 'host devices grant dev_1 --permission approve';
