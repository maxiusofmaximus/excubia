import 'dart:async';
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/state/chat_controller.dart';
import 'package:herdr_notify/core/state/live_triage.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';

import 'support/fake_chat_transport.dart';
import 'support/signal_triage.dart';

LiveTriage dummyTriage() => LiveTriage(
  HostClient(
    baseUrl: 'http://100.64.0.1:8788',
    deviceId: 'dev-x',
    privateKeyBase64: 'placeholder',
  ),
);

Map<String, String> routes() => {
  '/v1/chat/bootstrap': bootstrapJson(),
  '/v1/chat/sections': sectionsJson(const []),
  '/v1/chat/conversations': conversationsJson(
    conversations: [
      {
        'id': 'cnv_1',
        'title': 'Plan semanal',
        'pinned': true,
        'archived': false,
        'last_seq': 2,
        'last_read_seq': 0,
        'unread': 1,
        'last_message_at': '2026-10-22T10:00:00.000Z',
        'created_at': '2026-10-22T10:00:00.000Z',
        'updated_at': '2026-10-22T10:00:00.000Z',
      },
    ],
  ),
  '/v1/chat/conversations/cnv_1/timeline': timelineJson([
    timelineMessage(seq: 1, id: 'm1', preview: 'Crea el plan'),
    timelineMessage(
      seq: 2,
      id: 'm2',
      preview: 'Alfred: listo',
      author: 'agent',
    ),
  ]),
  '/v1/chat/conversations/cnv_1/messages': messagesJson([
    userMessage(seq: 1, id: 'm1', body: 'Crea el plan'),
    agentMessage(seq: 2, id: 'm2', body: 'Alfred: listo'),
  ]),
  '/v1/chat/conversations/cnv_1/read': readJson(2),
};

/// The same conversation with a turn open: its list row carries `active_runs`.
Map<String, String> runningRoutes() => routes()
  ..['/v1/chat/conversations'] = conversationsJson(
    conversations: [
      {
        'id': 'cnv_1',
        'title': 'Plan semanal',
        'pinned': true,
        'archived': false,
        'last_seq': 2,
        'last_read_seq': 0,
        'unread': 1,
        'active_runs': 1,
        'last_message_at': '2026-10-22T10:00:00.000Z',
        'created_at': '2026-10-22T10:00:00.000Z',
        'updated_at': '2026-10-22T10:00:00.000Z',
      },
    ],
  );

String stopAnswer({required bool stopped}) =>
    jsonEncode({'ok': true, 'stopped': stopped, 'runs': stopped ? 1 : 0});

/// Holds the answer of `POST …/messages` until the test releases it, so the
/// turn's frames can land before the `202`.
class _HeldSendTransport extends FakeChatTransport {
  _HeldSendTransport(super.routes);

  final Completer<void> release = Completer<void>();

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    if (method == 'POST' && target.contains('/messages')) await release.future;
    return super.call(method, target, body);
  }
}

/// The `chat.message` frame that moves the sent message («m3») to [state].
ChatEvent turnFrame(String state) => parseChatEvent(
  json:
      '{"event":"chat.message","revision":12,"conversation_id":"cnv_1",'
      '"message_id":"m3","seq":3,"change":"updated","turn_state":"$state"}',
)!;

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  test(
    'start opens the newest conversation and merges bodies with the timeline',
    () async {
      final transport = FakeChatTransport(routes());
      final controller = ChatController(transport, dummyTriage());
      addTearDown(controller.dispose);

      await controller.start();

      expect(controller.current?.id, 'cnv_1');
      expect(controller.entries.map((entry) => entry.seq), [1, 2]);
      expect(controller.entries[1].body, 'Alfred: listo');
      expect(controller.entries[1].authorKind, 'agent');
      expect(
        transport.calls.any(
          (call) => call.startsWith('POST /v1/chat/conversations/cnv_1/read'),
        ),
        isTrue,
      );
    },
  );

  test('send shows the optimistic bubble then the accepted seq without a duplicate', () async {
    final transport = FakeChatTransport(routes());
    transport.routes['POST /v1/chat/conversations/cnv_1/messages'] =
        acceptedJson(seq: 3, id: 'm3', body: 'Nuevo encargo');
    final controller = ChatController(transport, dummyTriage());
    addTearDown(controller.dispose);
    await controller.start();

    await controller.send('  Nuevo encargo  ');

    final entries = controller.entries;
    expect(entries.last.seq, 3);
    expect(entries.last.body, 'Nuevo encargo');
    expect(entries.last.local, isFalse);
    expect(
      entries.where((entry) => entry.body == 'Nuevo encargo'),
      hasLength(1),
    );
    final sent = transport.calls.indexWhere(
      (call) => call == 'POST /v1/chat/conversations/cnv_1/messages',
    );
    expect(sent, isNot(-1));
    final body = jsonDecode(utf8.decode(transport.bodies[sent])) as Map;
    expect(body['client_id'], isNotEmpty);
    expect(body['text'], 'Nuevo encargo');
  });

  test('a transport failure keeps the client_id for a safe retry', () async {
    final transport = FakeChatTransport(routes());
    transport.routes['POST /v1/chat/conversations/cnv_1/messages'] =
        acceptedJson(seq: 3, id: 'm3', body: 'hola');
    final controller = ChatController(transport, dummyTriage());
    addTearDown(controller.dispose);
    await controller.start();

    transport.failure = const HostError(0, 'no route');
    await controller.send('hola');
    final failed = controller.failedLocal;
    expect(failed, isNotNull);
    expect(failed!.state, 'failed');
    final firstIndex = transport.calls.indexWhere(
      (call) => call == 'POST /v1/chat/conversations/cnv_1/messages',
    );
    final firstBody =
        jsonDecode(utf8.decode(transport.bodies[firstIndex])) as Map;

    transport.failure = null;
    await controller.retryLocal(failed);
    final retryIndex = transport.calls.lastIndexWhere(
      (call) => call == 'POST /v1/chat/conversations/cnv_1/messages',
    );
    final retryBody =
        jsonDecode(utf8.decode(transport.bodies[retryIndex])) as Map;
    expect(retryBody['client_id'], firstBody['client_id']);
    expect(controller.failedLocal, isNull);
  });

  test(
    'a reply sends reply_to and the quote is dropped once accepted',
    () async {
      final transport = FakeChatTransport(routes());
      transport.routes['POST /v1/chat/conversations/cnv_1/messages'] =
          acceptedJson(seq: 3, id: 'm3', body: 'Sí, créalo');
      final controller = ChatController(transport, dummyTriage());
      addTearDown(controller.dispose);
      await controller.start();
      final target = controller.entries[1];

      controller.startReply(target);
      await controller.send('Sí, créalo', replyTo: target);

      final sent = transport.calls.indexWhere(
        (call) => call == 'POST /v1/chat/conversations/cnv_1/messages',
      );
      final body = jsonDecode(utf8.decode(transport.bodies[sent])) as Map;
      expect(body['reply_to'], {'kind': 'message', 'id': 'm2'});
      expect(controller.replyTo, isNull);
    },
  );

  test('a refused reply drops its quote and keeps no failed bubble', () async {
    final transport = FakeChatTransport(routes());
    final controller = ChatController(transport, dummyTriage());
    addTearDown(controller.dispose);
    await controller.start();
    final target = controller.entries[1];
    controller.startReply(target);

    transport.failure = const HostError(
      422,
      '{"detail":{"code":"unknown_reply_to","field":"reply_to","message":"x"}}',
    );
    await controller.send('Sí, créalo', replyTo: target);

    expect(controller.failure?.code, 'unknown_reply_to');
    expect(controller.replyTo, isNull);
    expect(controller.entries.where((entry) => entry.local), isEmpty);
  });

  test('a retried reply keeps the quote its first attempt named', () async {
    final transport = FakeChatTransport(routes());
    transport.routes['POST /v1/chat/conversations/cnv_1/messages'] =
        acceptedJson(seq: 3, id: 'm3', body: 'Sí, créalo');
    final controller = ChatController(transport, dummyTriage());
    addTearDown(controller.dispose);
    await controller.start();
    final target = controller.entries[1];

    transport.failure = const HostError(0, 'no route');
    await controller.send('Sí, créalo', replyTo: target);
    transport.failure = null;
    await controller.retryLocal(controller.failedLocal!);

    final retried = transport.calls.lastIndexWhere(
      (call) => call == 'POST /v1/chat/conversations/cnv_1/messages',
    );
    final body = jsonDecode(utf8.decode(transport.bodies[retried])) as Map;
    expect(body['reply_to'], {'kind': 'message', 'id': 'm2'});
  });

  test('search asks the host with the encoded query', () async {
    final transport = FakeChatTransport(routes());
    final controller = ChatController(
      transport,
      dummyTriage(),
      searchDelay: const Duration(milliseconds: 1),
    );
    addTearDown(controller.dispose);
    await controller.start();
    transport.calls.clear();

    controller.setQuery('plan de la api');
    await Future<void>.delayed(const Duration(milliseconds: 20));

    expect(
      transport.calls.any((call) => call.contains('q=plan%20de%20la%20api')),
      isTrue,
    );
  });

  test('archiving the open conversation clears the timeline', () async {
    final transport = FakeChatTransport(routes());
    transport.routes['POST /v1/chat/conversations/cnv_1/update'] = jsonEncode({
      'ok': true,
      'conversation': {
        'id': 'cnv_1',
        'title': 'Plan semanal',
        'pinned': true,
        'archived': true,
        'last_seq': 2,
        'last_read_seq': 0,
        'unread': 1,
        'last_message_at': '2026-10-22T10:00:00.000Z',
        'created_at': '2026-10-22T10:00:00.000Z',
        'updated_at': '2026-10-22T10:00:00.000Z',
      },
    });
    final controller = ChatController(transport, dummyTriage());
    addTearDown(controller.dispose);
    await controller.start();
    final conversation = controller.conversations.single;

    expect(await controller.archive(conversation), isNull);
    expect(controller.current, isNull);
    expect(controller.entries, isEmpty);
  });

  test(
    'a delegation detail loads its result and cancel updates the card',
    () async {
      final transport = FakeChatTransport(routes());
      transport.routes['/v1/chat/conversations/cnv_1/timeline'] = timelineJson([
        timelineMessage(seq: 1, id: 'm1', preview: 'Crea el proyecto Demo'),
        timelineCard(
          seq: 2,
          delegationId: 'dlg_1',
          title: 'Alfred pidió a Ernesto',
          status: 'running',
          summary: 'Ernesto planifica',
          paneId: 'w1:p3',
        ),
      ]);
      transport.routes['/v1/chat/delegations/dlg_1'] = delegationDetailJson(
        id: 'dlg_1',
        result: 'Plan de tres hitos',
      );
      transport.routes['POST /v1/chat/delegations/dlg_1/cancel'] =
          delegationDetailJson(id: 'dlg_1', state: 'cancelled');
      final controller = ChatController(transport, dummyTriage());
      addTearDown(controller.dispose);
      await controller.start();

      final card = controller.entries.firstWhere(
        (entry) => entry.kind == 'card',
      );
      expect(card.kind, 'card');
      expect(card.delegationId, 'dlg_1');
      expect(card.paneId, 'w1:p3');

      final detail = await controller.delegationDetail('dlg_1');
      expect(detail.failure, isNull);
      expect(detail.detail?.toName, 'Ernesto');
      expect(detail.detail?.resultBody, 'Plan de tres hitos');
      expect(detail.detail?.children.single.toName, 'Bob');

      expect(await controller.cancelDelegation('dlg_1'), isNull);
      final cancelled = controller.entries.firstWhere(
        (entry) => entry.kind == 'card',
      );
      expect(cancelled.status, 'cancelled');
      expect(
        transport.calls,
        contains('POST /v1/chat/delegations/dlg_1/cancel'),
      );
    },
  );

  test('stop sends one request per turn, and only while a turn runs', () async {
    final transport = FakeChatTransport(runningRoutes());
    transport.routes['POST /v1/chat/conversations/cnv_1/stop'] = stopAnswer(
      stopped: true,
    );
    final controller = ChatController(transport, dummyTriage());
    addTearDown(controller.dispose);
    await controller.start();

    expect(controller.turnInFlight, isTrue);
    final taps = await Future.wait([controller.stop(), controller.stop()]);
    expect(taps, [isNull, isNull]);
    expect(
      transport.calls.where((call) => call.endsWith('/stop')),
      hasLength(1),
    );
  });

  test('a stop with no open turn sends nothing', () async {
    final transport = FakeChatTransport(routes());
    final controller = ChatController(transport, dummyTriage());
    addTearDown(controller.dispose);
    await controller.start();

    expect(controller.turnInFlight, isFalse);
    expect(await controller.stop(), isNull);
    expect(transport.calls.where((call) => call.endsWith('/stop')), isEmpty);
  });

  test('a stop that finds the turn already done is not an error', () async {
    final transport = FakeChatTransport(runningRoutes());
    transport.routes['POST /v1/chat/conversations/cnv_1/stop'] = stopAnswer(
      stopped: false,
    );
    final controller = ChatController(transport, dummyTriage());
    addTearDown(controller.dispose);
    await controller.start();

    expect(await controller.stop(), isNull);
    expect(controller.stopDenial, isNull);
  });

  test('a refused stop names its cause and a denied device waits', () async {
    final transport = FakeChatTransport(runningRoutes());
    final controller = ChatController(transport, dummyTriage());
    addTearDown(controller.dispose);
    await controller.start();

    transport.failure = const HostError(403, '{"detail":"forbidden"}');
    final denied = await controller.stop();
    expect(
      denied?.message,
      'No tienes permiso para detener esto desde este dispositivo.',
    );
    expect(controller.stopDenial, same(denied));
    expect(controller.canStop, isFalse);
    expect(controller.failure, isNull, reason: 'a refusal is not a card');

    final sent = transport.calls.length;
    expect(await controller.stop(), isNull);
    expect(transport.calls, hasLength(sent), reason: 'a denied device waits');
  });

  test('an old host and a lost network each get their own sentence', () async {
    final transport = FakeChatTransport(runningRoutes());
    final controller = ChatController(transport, dummyTriage());
    addTearDown(controller.dispose);
    await controller.start();

    transport.failure = const HostError(404, '{"detail":"Not Found"}');
    expect(
      (await controller.stop())?.message,
      'Este equipo no admite detener todavía.',
    );
    transport.failure = const HostError(0, '');
    expect(
      (await controller.stop())?.message,
      'No se pudo detener. Inténtalo de nuevo.',
    );
    expect(controller.stopDenial, isNull);
  });

  test('bootstrap projects keep the lead for the team surface', () async {
    final transport = FakeChatTransport(routes());
    transport.routes['/v1/chat/bootstrap'] = bootstrapJson(
      projects: [
        {
          'id': 'prj_1',
          'name': 'Demo',
          'root_label': 'demo',
          'status': 'active',
          'lead': {
            'agent_id': 'ag_1',
            'name': 'Ernesto',
            'status': 'working',
            'pane_id': 'w1:p3',
          },
        },
      ],
    );
    final controller = ChatController(transport, dummyTriage());
    addTearDown(controller.dispose);
    await controller.start();

    final project = controller.projects.single;
    expect(project.name, 'Demo');
    expect(project.lead?.name, 'Ernesto');
    expect(project.lead?.paneId, 'w1:p3');
  });

  test('a turn that ends before its 202 is closed, though the list still counts it', () async {
    final triage = SignalTriage();
    addTearDown(triage.signals.close);
    final transport = _HeldSendTransport(runningRoutes());
    transport.routes['POST /v1/chat/conversations/cnv_1/messages'] =
        acceptedJson(seq: 3, id: 'm3', body: 'Nuevo encargo');
    final controller = ChatController(transport, triage);
    addTearDown(controller.dispose);
    await controller.start();

    final sent = controller.send('Nuevo encargo');
    await Future<void>.delayed(const Duration(milliseconds: 10));
    triage.signals.add(turnFrame('running'));
    triage.signals.add(turnFrame('completed'));
    await Future<void>.delayed(const Duration(milliseconds: 10));
    transport.release.complete();
    await sent;

    expect(controller.entries.last.turnState, 'completed');
    expect(controller.turnInFlight, isFalse);
  });

  test('a lost turn frame is read back by the next list refresh', () async {
    final triage = SignalTriage();
    addTearDown(triage.signals.close);
    final transport = FakeChatTransport(routes());
    transport.routes['POST /v1/chat/conversations/cnv_1/messages'] =
        acceptedJson(seq: 3, id: 'm3', body: 'Nuevo encargo');
    final controller = ChatController(transport, triage);
    addTearDown(controller.dispose);
    await controller.start();

    await controller.send('Nuevo encargo');
    expect(controller.turnInFlight, isTrue);
    // No frame arrives, and the timeline still says queued.
    await Future<void>.delayed(const Duration(milliseconds: 10));

    transport.routes['/v1/chat/conversations/cnv_1/timeline'] = timelineJson([
      timelineMessage(seq: 1, id: 'm1', preview: 'Crea el plan'),
      timelineMessage(
        seq: 2,
        id: 'm2',
        preview: 'Alfred: listo',
        author: 'agent',
      ),
      timelineMessage(
        seq: 3,
        id: 'm3',
        preview: 'Nuevo encargo',
        turnState: 'completed',
      ),
    ]);
    await controller.loadHistory();
    await Future<void>.delayed(const Duration(milliseconds: 10));

    expect(controller.entries.last.turnState, 'completed');
    expect(controller.turnInFlight, isFalse);
  });

  test('send says whether the host took the message, for a voice note to fall back on', () async {
    final transport = FakeChatTransport(routes());
    transport.routes['POST /v1/chat/conversations/cnv_1/messages'] =
        acceptedJson(seq: 3, id: 'm3', body: 'Nuevo encargo');
    final controller = ChatController(transport, dummyTriage());
    addTearDown(controller.dispose);
    await controller.start();

    expect(await controller.send('Nuevo encargo'), isTrue);

    transport.failure = const HostError(0, 'no route');
    expect(await controller.send('otro'), isFalse);
    expect(await controller.send('   '), isFalse);
  });
}
