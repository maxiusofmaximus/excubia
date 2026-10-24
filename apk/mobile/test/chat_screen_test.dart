import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/core/state/chat_controller.dart';
import 'package:herdr_notify/core/state/live_triage.dart';
import 'package:herdr_notify/src/rust/api/error.dart' show FailureKind;
import 'package:herdr_notify/src/rust/api/error/chat.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/chat/chat_bubbles.dart';
import 'package:herdr_notify/ui/chat/chat_screen.dart';
import 'package:herdr_notify/ui/chat/history_drawer.dart';
import 'package:herdr_notify/ui/theme.dart';

import 'support/fake_chat_transport.dart';

LiveTriage dummyTriage() => LiveTriage(
  GatewayClient(
    baseUrl: 'http://100.64.0.1:8788',
    deviceId: 'dev-x',
    privateKeyBase64: 'placeholder',
  ),
);

Map<String, String> emptyRoutes() => {
  '/v1/chat/bootstrap': bootstrapJson(),
  '/v1/chat/sections': sectionsJson(const []),
  '/v1/chat/conversations': conversationsJson(),
};

Map<String, String> conversationRoutes() => {
  '/v1/chat/bootstrap': bootstrapJson(),
  '/v1/chat/sections': sectionsJson(const []),
  '/v1/chat/conversations': conversationsJson(
    conversations: [
      {
        'id': 'cnv_1',
        'title': 'Plan semanal',
        'pinned': false,
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

Future<ChatController> pumpChat(
  WidgetTester tester, {
  required Map<String, String> routes,
}) async {
  final transport = FakeChatTransport(routes);
  final controller = ChatController(transport, dummyTriage());
  await controller.start();
  addTearDown(controller.dispose);
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      home: ChatScreen(controller: controller, onOpenTeam: () {}),
    ),
  );
  await tester.pumpAndSettle();
  return controller;
}

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  testWidgets('the empty start screen invites the first goal', (tester) async {
    await pumpChat(tester, routes: emptyRoutes());

    expect(find.text('¿Qué movemos hoy?'), findsOneWidget);
    expect(find.text('Crea el proyecto Demo'), findsOneWidget);
    expect(find.byTooltip('Nuevo chat'), findsOneWidget);
    expect(find.text('Mensaje a Alfred'), findsOneWidget);
  });

  testWidgets('a conversation renders both messages and the composer', (
    tester,
  ) async {
    await pumpChat(tester, routes: conversationRoutes());

    expect(find.text('Crea el plan'), findsOneWidget);
    expect(find.text('Alfred: listo'), findsOneWidget);
    final field = tester.widget<TextField>(find.byType(TextField));
    expect(field.enabled, isTrue);
  });

  testWidgets('a failed turn is a card, never an invented reply', (
    tester,
  ) async {
    final routes = conversationRoutes();
    routes['/v1/chat/conversations/cnv_1/timeline'] = timelineJson([
      timelineMessage(seq: 1, id: 'm1', preview: 'Resume el estado'),
      timelineMessage(
        seq: 2,
        id: 'm2',
        preview: '',
        author: 'agent',
        state: 'failed',
      ),
    ]);
    routes['/v1/chat/conversations/cnv_1/messages'] = messagesJson([
      userMessage(seq: 1, id: 'm1', body: 'Resume el estado'),
      agentMessage(seq: 2, id: 'm2', body: '', state: 'failed'),
    ]);
    await pumpChat(tester, routes: routes);

    expect(find.text('Alfred terminó sin responder'), findsOneWidget);
    expect(find.textContaining('no se inventó'), findsOneWidget);
    expect(find.text('Reintentar'), findsOneWidget);
  });

  testWidgets('offline, missing seat, busy queue and 403 have distinct cards', (
    tester,
  ) async {
    for (final failure in [
      ChatFailure(
        code: 'offline',
        shownInTimeline: true,
        keptByHost: false,
        message: 'Sin conexión con el equipo.',
        action: 'Reconecta para escribir a Alfred.',
        kind: FailureKind.retry,
      ),
      ChatFailure(
        code: 'seat_unavailable',
        shownInTimeline: true,
        keptByHost: true,
        message:
            'Alfred no pudo arrancar: el asiento no tiene un CLI disponible.',
        action: 'Revisa el motor y el CLI en el host.',
        kind: FailureKind.retry,
      ),
      ChatFailure(
        code: 'agent_busy',
        shownInTimeline: true,
        keptByHost: true,
        message: 'La cola de Alfred está llena.',
        action: 'Reintenta en 18 s; el mensaje no se envió.',
        kind: FailureKind.retry,
        retryAfter: 18,
      ),
      ChatFailure(
        code: 'forbidden',
        shownInTimeline: true,
        keptByHost: false,
        message: 'Falta el permiso RESPOND para actuar sobre el agente.',
        action: 'Concédelo en el host: host devices grant <device_id> --permission respond',
        kind: FailureKind.permission,
      ),
    ]) {
      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(),
          home: Scaffold(
            body: ChatFailureCard(
              failure: failure,
              onRetry: () {},
              onDismiss: () {},
            ),
          ),
        ),
      );
      expect(find.text(failure.message), findsOneWidget);
      if (failure.code == 'forbidden') {
        expect(find.text('Copiar comando'), findsOneWidget);
        expect(find.textContaining('--permission respond'), findsOneWidget);
      } else if (failure.code == 'offline') {
        expect(find.text('Reconectar'), findsOneWidget);
      } else {
        expect(find.text('Reintentar'), findsOneWidget);
      }
    }
  });

  testWidgets(
    'the history drawer lists sections and asks the host for a filter',
    (tester) async {
      final routes = conversationRoutes();
      routes['/v1/chat/sections'] = sectionsJson([
        {'id': 'sec_1', 'name': 'Trabajo', 'order': 1},
      ]);
      routes['/v1/chat/conversations'] = conversationsJson(
        conversations: [
          {
            'id': 'cnv_1',
            'title': 'Plan semanal',
            'pinned': true,
            'section_id': 'sec_1',
            'archived': false,
            'last_seq': 2,
            'last_read_seq': 0,
            'unread': 1,
            'last_message_at': '2026-10-22T10:00:00.000Z',
            'created_at': '2026-10-22T10:00:00.000Z',
            'updated_at': '2026-10-22T10:00:00.000Z',
          },
        ],
      );
      final transport = FakeChatTransport(routes);
      final controller = ChatController(transport, dummyTriage());
      await controller.start();
      addTearDown(controller.dispose);
      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(),
          home: Scaffold(
            body: HistoryDrawer(controller: controller, onOpenTeam: () {}),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Historial'), findsOneWidget);
      expect(find.text('Plan semanal'), findsOneWidget);
      expect(find.text('Secciones'), findsOneWidget);

      await tester.tap(find.text('Fijados'));
      await tester.pumpAndSettle();
      expect(
        transport.calls.any((call) => call.contains('pinned=true')),
        isTrue,
      );
    },
  );
}
