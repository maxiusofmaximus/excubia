import 'dart:async';
import 'dart:convert';

import 'package:excubia/core/api/chat_attachment_source.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/state/chat_attachment_tray.dart';
import 'package:excubia/core/state/live_triage.dart';

import 'fake_chat_transport.dart';

LiveTriage dummyTriage() => LiveTriage(
  HostClient(
    baseUrl: 'http://100.64.0.1:8788',
    deviceId: 'dev-x',
    privateKeyBase64: 'placeholder',
  ),
);

/// An in-memory picked file. The sha256 is fixed: the fake host never checks it.
class MemorySource implements ChatAttachmentSource {
  MemorySource(this.name, this.bytes);

  @override
  final String name;
  final List<int> bytes;

  @override
  int get size => bytes.length;

  @override
  Future<List<int>> read(int offset, int length) async =>
      bytes.sublist(offset, (offset + length).clamp(0, bytes.length));

  @override
  Future<String?> sha256() async => List.filled(64, 'a').join();
}

/// A PNG: the magic bytes and a few more, so the sniff passes.
MemorySource png([String name = 'captura.png']) => MemorySource(name, const [
  0x89,
  0x50,
  0x4E,
  0x47,
  0x0D,
  0x0A,
  0x1A,
  0x0A,
  0,
  0,
  0,
  0,
]);

/// The host's answer to create, chunk-less complete and delete for `att_1`.
String attachmentJson({required String state}) => jsonEncode({
  'ok': true,
  'attachment': {
    'id': 'att_1',
    'name': 'captura.png',
    'mime': 'image/webp',
    'size': 12,
    'w': 2,
    'h': 3,
    'state': state,
  },
});

/// Chat routes plus the upload ones. The chunks are answered by [UploadTransport].
Map<String, String> chatRoutes() => {
  '/v1/chat/bootstrap': bootstrapJson(),
  '/v1/chat/sections': sectionsJson(const []),
  '/v1/chat/conversations': conversationsJson(
    conversations: [
      {
        'id': 'cnv_1',
        'title': 'Plan semanal',
        'pinned': true,
        'archived': false,
        'last_seq': 1,
        'last_read_seq': 0,
        'unread': 0,
        'last_message_at': '2026-10-22T10:00:00.000Z',
        'created_at': '2026-10-22T10:00:00.000Z',
        'updated_at': '2026-10-22T10:00:00.000Z',
      },
    ],
  ),
  '/v1/chat/conversations/cnv_1/timeline': timelineJson([
    timelineMessage(seq: 1, id: 'm1', preview: 'Crea el plan'),
  ]),
  '/v1/chat/conversations/cnv_1/messages': messagesJson([
    userMessage(seq: 1, id: 'm1', body: 'Crea el plan'),
  ]),
  '/v1/chat/conversations/cnv_1/read': readJson(1),
  'POST /v1/chat/conversations/cnv_1/attachments': attachmentJson(
    state: 'uploading',
  ),
  'POST /v1/chat/conversations/cnv_1/attachments/att_1/complete':
      attachmentJson(state: 'ready'),
  'DELETE /v1/chat/conversations/cnv_1/attachments/att_1': jsonEncode({
    'ok': true,
  }),
  'POST /v1/chat/conversations/cnv_1/messages': acceptedJson(
    seq: 3,
    id: 'm3',
    body: '',
  ),
};

/// Answers the chunk posts from the bytes they carry, so the offsets are real.
/// [chunkRefusals] are thrown in order before the answers resume.
class UploadTransport extends FakeChatTransport {
  UploadTransport(super.routes);

  final List<HostError> chunkRefusals = <HostError>[];
  final List<int> chunkOffsets = <int>[];

  /// Holds the create answer until the test completes it.
  Completer<void>? holdCreate;

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    if (method == 'POST' && target.endsWith('/attachments')) {
      await holdCreate?.future;
    }
    if (method != 'POST' || !target.endsWith('/chunks')) {
      return super.call(method, target, body);
    }
    calls.add('$method $target');
    bodies.add(body);
    final sent = jsonDecode(utf8.decode(body)) as Map<String, Object?>;
    final offset = sent['offset']! as int;
    chunkOffsets.add(offset);
    if (chunkRefusals.isNotEmpty) throw chunkRefusals.removeAt(0);
    final length = base64Decode(sent['data']! as String).length;
    return jsonEncode({'ok': true, 'next_offset': offset + length});
  }
}

/// Lets queued uploads run to their end, or gives up after a few real seconds.
/// The Rust calls cross a real port, so a count of turns is not enough.
Future<void> settle(ChatAttachmentTray tray) async {
  final deadline = DateTime.now().add(const Duration(seconds: 5));
  while (tray.items.any(_unfinished) && DateTime.now().isBefore(deadline)) {
    await Future<void>.delayed(const Duration(milliseconds: 1));
  }
}

bool _unfinished(AttachmentDraft draft) =>
    draft.status == DraftStatus.queued || draft.status == DraftStatus.uploading;

Future<void> noPause(Duration _) async {}
