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
  String seatId = 'seat_bob',
  String tool = 'bash',
  Map<String, Object?>? details,
}) => {
  'id': id,
  'conversation_id': conversationId,
  'seat_id': seatId,
  'pane_id': 'w1:p9',
  'kind': 'command',
  'tool': tool,
  'summary': 'Ejecutar comando: $command',
  'cwd': 'demo/api',
  'risk': 'medium',
  'status': status,
  'level': level,
  'expires_at': expiresAt ?? soonIso(),
  'payload_redacted': {'command': command, 'question': ?question},
  'details': ?details,
};

/// One mail of a `mail.move` batch, with the facts the host sends to `APPROVE`.
Map<String, Object?> moveItem(
  String messageId, {
  String dest = 'junkemail',
  String? fromAddress = 'oferta@ejemplo.org',
  String subject = 'Gana dinero',
  String reason = 'Phishing probable',
  String? level = 'phish_candidate',
  bool excluded = false,
}) => {
  'message_id': messageId,
  'dest': dest,
  'reason': reason,
  'from_address': ?fromAddress,
  'subject': subject,
  'level': ?level,
  'excluded': excluded,
};

/// A `mail.move` approval row whose details carry [items].
Map<String, Object?> moveRow({
  String status = 'pending',
  List<Map<String, Object?>>? items,
}) => approvalRow(
  tool: 'mail.move',
  status: status,
  details: {
    'account': 'trabajo',
    'items': items ?? [moveItem('m1'), moveItem('m2', dest: 'inbox')],
  },
);

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
