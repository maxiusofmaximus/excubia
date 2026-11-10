import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/state/chat_controller.dart';
import 'package:excubia/src/rust/api/chat/model.dart';
import 'package:excubia/src/rust/frb_generated.dart';

import 'support/fake_chat_transport.dart';
import 'support/scripted_transport.dart';
import 'support/signal_triage.dart';

const _deletePath = 'DELETE /v1/chat/conversations/cnv_1';

Map<String, Object?> conversation({
  bool pinned = false,
  bool archived = false,
}) => {
  'id': 'cnv_1',
  'title': 'Plan semanal',
  'pinned': pinned,
  'archived': archived,
  'last_seq': 1,
  'last_read_seq': 0,
  'unread': 0,
  'last_message_at': '2026-10-22T10:00:00.000Z',
  'created_at': '2026-10-22T10:00:00.000Z',
  'updated_at': '2026-10-22T10:00:00.000Z',
};

Map<String, String> routes({bool pinned = false, bool archived = false}) => {
  '/v1/chat/bootstrap': bootstrapJson(),
  '/v1/chat/sections': sectionsJson(const []),
  '/v1/chat/conversations': conversationsJson(
    conversations: [conversation(pinned: pinned, archived: archived)],
  ),
  '/v1/chat/conversations/cnv_1/timeline': timelineJson([
    timelineMessage(seq: 1, id: 'm1', preview: 'Crea el plan'),
  ]),
  '/v1/chat/conversations/cnv_1/messages': messagesJson([
    userMessage(seq: 1, id: 'm1', body: 'Crea el plan'),
  ]),
  '/v1/chat/conversations/cnv_1/read': readJson(1),
  _deletePath: jsonEncode({'ok': true}),
};

/// The host no longer lists the conversation once it is gone.
void forgetOnHost(ScriptedTransport transport) {
  transport.routes['/v1/chat/conversations'] = conversationsJson();
}

ChatEvent deletedFrame(String conversationId) => parseChatEvent(
  json:
      '{"event":"chat.conversation","revision":13,'
      '"conversation_id":"$conversationId","change":"deleted"}',
)!;

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  test(
    'deleting the open conversation drops it and clears the timeline',
    () async {
      final transport = ScriptedTransport(routes());
      final controller = ChatController(transport, SignalTriage());
      addTearDown(controller.dispose);
      await controller.start();
      final target = controller.conversations.single;
      forgetOnHost(transport);

      expect(await controller.deleteConversation(target), isNull);

      expect(transport.calls, contains(_deletePath));
      expect(controller.conversations, isEmpty);
      expect(controller.current, isNull);
      expect(controller.entries, isEmpty);
    },
  );

  test(
    'the pinned and archived counts follow the deleted conversation',
    () async {
      final transport = ScriptedTransport(routes(pinned: true, archived: true));
      final controller = ChatController(transport, SignalTriage());
      addTearDown(controller.dispose);
      await controller.start();
      await controller.loadHistory();
      expect(controller.pinnedCount, 1);
      expect(controller.archivedCount, 1);
      final target = controller.conversations.single;
      forgetOnHost(transport);

      await controller.deleteConversation(target);

      expect(controller.pinnedCount, 0);
      expect(controller.archivedCount, 0);
    },
  );

  test('a busy conversation stays and says what to stop first', () async {
    final transport = ScriptedTransport(routes());
    transport.failNext(
      _deletePath,
      const HostError(409, '{"detail":"conversation_busy"}'),
    );
    final controller = ChatController(transport, SignalTriage());
    addTearDown(controller.dispose);
    await controller.start();

    final problem = await controller.deleteConversation(
      controller.conversations.single,
    );

    expect(problem?.code, 'conversation_busy');
    expect(problem?.message, endsWith('antes de borrarla.'));
    expect(controller.conversations, hasLength(1));
    expect(controller.current?.id, 'cnv_1');
  });

  test('a conversation the host no longer has counts as deleted', () async {
    final transport = ScriptedTransport(routes());
    transport.failNext(
      _deletePath,
      const HostError(404, '{"detail":"no_such_conversation"}'),
    );
    final controller = ChatController(transport, SignalTriage());
    addTearDown(controller.dispose);
    await controller.start();

    expect(
      await controller.deleteConversation(controller.conversations.single),
      isNull,
    );
    expect(controller.conversations, isEmpty);
    expect(controller.current, isNull);
  });

  test(
    'a refused connection keeps the conversation and reads as offline',
    () async {
      final transport = ScriptedTransport(routes());
      transport.failNext(_deletePath, const HostError(0, ''));
      final controller = ChatController(transport, SignalTriage());
      addTearDown(controller.dispose);
      await controller.start();

      final problem = await controller.deleteConversation(
        controller.conversations.single,
      );

      expect(problem?.code, 'offline');
      expect(problem?.message, 'Sin conexión con el equipo.');
      expect(controller.conversations, hasLength(1));
    },
  );

  test(
    'a deleted frame from another app removes the open conversation',
    () async {
      final triage = SignalTriage();
      addTearDown(triage.signals.close);
      final transport = ScriptedTransport(routes());
      final controller = ChatController(transport, triage);
      addTearDown(controller.dispose);
      await controller.start();
      forgetOnHost(transport);

      triage.signals.add(deletedFrame('cnv_1'));
      await Future<void>.delayed(const Duration(milliseconds: 10));

      expect(controller.conversations, isEmpty);
      expect(controller.current, isNull);
    },
  );

  test('a deleted frame for an unknown conversation changes nothing', () async {
    final triage = SignalTriage();
    addTearDown(triage.signals.close);
    final controller = ChatController(ScriptedTransport(routes()), triage);
    addTearDown(controller.dispose);
    await controller.start();

    triage.signals.add(deletedFrame('cnv_gone'));
    await Future<void>.delayed(const Duration(milliseconds: 10));

    expect(controller.conversations, hasLength(1));
    expect(controller.current?.id, 'cnv_1');
  });
}
