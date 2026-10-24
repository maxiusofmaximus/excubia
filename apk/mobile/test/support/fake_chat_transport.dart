import 'dart:convert';

import 'package:herdr_notify/core/api/chat_transport.dart';

/// Routes canned chat responses by path, ignoring the query. Tests assert the
/// routes the controller asks for without a socket or a signed key.
class FakeChatTransport implements ChatTransport {
  FakeChatTransport(this.routes);

  final Map<String, String> routes;
  final List<String> calls = <String>[];
  final List<List<int>> bodies = <List<int>>[];
  Object? failure;

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    calls.add('$method $target');
    bodies.add(body);
    final error = failure;
    if (error != null) throw error;
    final path = target.split('?').first;
    final response = routes['$method $path'] ?? routes[path];
    if (response == null) {
      throw StateError('no canned response for $method $path');
    }
    return response;
  }
}

String conversationsJson({
  List<Map<String, Object?>> conversations = const [],
  String? nextCursor,
}) => jsonEncode({
  'ok': true,
  'conversations': conversations,
  'next_cursor': ?nextCursor,
});

String conversationJson({required String id, String title = 'Nuevo chat'}) =>
    jsonEncode({
      'ok': true,
      'conversation': {
        'id': id,
        'title': title,
        'pinned': false,
        'archived': false,
        'last_seq': 0,
        'last_read_seq': 0,
        'unread': 0,
        'last_message_at': '2026-10-22T10:00:00.000Z',
        'created_at': '2026-10-22T10:00:00.000Z',
        'updated_at': '2026-10-22T10:00:00.000Z',
      },
    });

String timelineJson(List<Map<String, Object?>> entries, {int? nextBefore}) =>
    jsonEncode({
      'ok': true,
      'entries': entries,
      'next_before_seq': ?nextBefore,
    });

String messagesJson(List<Map<String, Object?>> messages) =>
    jsonEncode({'ok': true, 'messages': messages});

Map<String, Object?> userMessage({
  required int seq,
  required String id,
  required String body,
}) => {
  'id': id,
  'seq': seq,
  'author_kind': 'user',
  'body_md': body,
  'state': 'complete',
  'created_at': '2026-10-22T10:00:00.000Z',
};

Map<String, Object?> agentMessage({
  required int seq,
  required String id,
  required String body,
  String state = 'complete',
}) => {
  'id': id,
  'seq': seq,
  'author_kind': 'agent',
  'body_md': body,
  'state': state,
  'created_at': '2026-10-22T10:00:00.000Z',
};

Map<String, Object?> timelineMessage({
  required int seq,
  required String id,
  required String preview,
  String author = 'user',
  String state = 'complete',
}) => {
  'seq': seq,
  'kind': 'message',
  'message_id': id,
  'author_kind': author,
  'preview': preview,
  'state': state,
  'created_at': '2026-10-22T10:00:00.000Z',
};

String bootstrapJson({
  String status = 'idle',
  List<Map<String, Object?>> projects = const [],
}) => jsonEncode({
  'ok': true,
  'alfred': {'name': 'Alfred', 'status': status},
  'unread': {'conversations': 0, 'messages': 0, 'approvals': 0},
  'chat': {'revision': 1, 'pending_approvals': 0},
  'projects': projects,
});

Map<String, Object?> timelineCard({
  required int seq,
  required String delegationId,
  required String title,
  required String status,
  String summary = '',
  String? paneId,
  int attention = 0,
}) => {
  'seq': seq,
  'kind': 'card',
  'card_id': 'act_$delegationId',
  'delegation_id': delegationId,
  'ref_id': delegationId,
  'title': title,
  'summary': summary,
  'status': status,
  'state': status,
  'pane_id': ?paneId,
  'attention': attention,
  'created_at': '2026-10-22T10:00:00.000Z',
};

String delegationDetailJson({
  required String id,
  String state = 'running',
  String result = '',
}) => jsonEncode({
  'ok': true,
  'delegation': {
    'id': id,
    'conversation_id': 'cnv_1',
    'project_id': 'prj_1',
    'from_agent_id': 'ag_alfred',
    'from_name': 'Alfred',
    'to_agent_id': 'ag_ernesto',
    'to_name': 'Ernesto',
    'task': 'Planifica el hito 2',
    'state': state,
    'depth': 1,
    'pane_id': 'w1:p3',
    'created_at': '2026-10-22T10:00:00.000Z',
    'updated_at': '2026-10-22T10:05:00.000Z',
  },
  'children': [
    {'id': 'dlg_2', 'depth': 2, 'state': 'done', 'to_name': 'Bob'},
  ],
  if (result.isNotEmpty)
    'result_message': {
      'id': 'msg_9',
      'body_md': result,
      'created_at': '2026-10-22T10:05:00.000Z',
    },
});

String sectionsJson(List<Map<String, Object?>> sections) =>
    jsonEncode({'ok': true, 'sections': sections});

String acceptedJson({
  required int seq,
  required String id,
  required String body,
}) => jsonEncode({
  'ok': true,
  'message': {
    'id': id,
    'conversation_id': 'cnv_1',
    'seq': seq,
    'author_kind': 'user',
    'body_md': body,
    'state': 'complete',
    'created_at': '2026-10-22T10:00:00.000Z',
  },
  'turn': {'id': 'run_1', 'state': 'queued', 'agent_id': 'ag_1'},
});

String readJson(int lastSeq) => jsonEncode({
  'ok': true,
  'conversation_id': 'cnv_1',
  'last_read_seq': lastSeq,
});
