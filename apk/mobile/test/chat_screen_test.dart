import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/state/chat_controller.dart';
import 'package:herdr_notify/core/state/hires_controller.dart';
import 'package:herdr_notify/core/state/live_triage.dart';
import 'package:herdr_notify/src/rust/api/error.dart' show FailureKind;
import 'package:herdr_notify/src/rust/api/error/chat.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/chat/chat_bubbles.dart';
import 'package:herdr_notify/ui/chat/chat_conversation_view.dart';
import 'package:herdr_notify/ui/chat/chat_screen.dart';
import 'package:herdr_notify/ui/chat/history_drawer.dart';
import 'package:herdr_notify/ui/theme.dart';
import 'package:herdr_notify/ui/widgets/surface_card.dart';

import 'support/fake_chat_transport.dart';
import 'support/fake_team_host.dart';
import 'support/history_chips.dart';

LiveTriage dummyTriage() => LiveTriage(
  HostClient(
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

/// An open turn whose list row carries the host's step, when it has one.
Map<String, String> stepRoutes(String? progress) => conversationRoutes()
  ..['/v1/chat/conversations'] = conversationsJson(
    conversations: [
      {
        'id': 'cnv_1',
        'title': 'Plan semanal',
        'pinned': false,
        'archived': false,
        'last_seq': 2,
        'last_read_seq': 0,
        'unread': 0,
        'active_runs': 1,
        'progress': ?progress,
        'last_message_at': '2026-10-22T10:00:00.000Z',
        'created_at': '2026-10-22T10:00:00.000Z',
        'updated_at': '2026-10-22T10:00:00.000Z',
      },
    ],
  )
  ..['/v1/chat/conversations/cnv_1/timeline'] = timelineJson([
    {
      ...timelineMessage(seq: 1, id: 'm1', preview: 'Crea el plan'),
      'turn_state': 'running',
    },
  ])
  ..['/v1/chat/conversations/cnv_1/messages'] = messagesJson([
    userMessage(seq: 1, id: 'm1', body: 'Crea el plan'),
  ]);

/// The conversation with a turn open: its list row carries `active_runs`.
Map<String, String> runningRoutes() => conversationRoutes()
  ..['/v1/chat/conversations'] = conversationsJson(
    conversations: [
      {
        'id': 'cnv_1',
        'title': 'Plan semanal',
        'pinned': false,
        'archived': false,
        'last_seq': 2,
        'last_read_seq': 0,
        'unread': 0,
        'active_runs': 1,
        'last_message_at': '2026-10-22T10:00:00.000Z',
        'created_at': '2026-10-22T10:00:00.000Z',
        'updated_at': '2026-10-22T10:00:00.000Z',
      },
    ],
  );

Future<ChatController> pumpChat(
  WidgetTester tester, {
  required Map<String, String> routes,
  FakeChatTransport? transport,
  HiresController? hires,
}) async {
  final chatTransport = transport ?? FakeChatTransport(routes);
  final controller = ChatController(chatTransport, dummyTriage());
  await controller.start();
  addTearDown(controller.dispose);
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      home: ChatScreen(
        controller: controller,
        hires: hires ?? HiresController(transport: chatTransport),
        onOpenTeam: () {},
        onUnpaired: () async {},
      ),
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

  testWidgets('the team card counts the hired people, not only the live ones', (
    tester,
  ) async {
    final hires = HiresController(transport: FakeTeamHost(hires: [bobHire()]));
    await hires.load();
    await pumpChat(tester, routes: emptyRoutes(), hires: hires);

    expect(find.text('1 en el equipo · 0 activos'), findsOneWidget);
    expect(find.text('Sin agentes'), findsNothing);
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

  testWidgets('a conversation opens at its last message, not its first', (
    tester,
  ) async {
    final routes = conversationRoutes();
    final numbered = [
      for (var n = 1; n <= 40; n++) 'Mensaje número $n de la conversación',
    ];
    routes['/v1/chat/conversations/cnv_1/timeline'] = timelineJson([
      for (var n = 1; n <= 40; n++)
        timelineMessage(seq: n, id: 'm$n', preview: numbered[n - 1]),
    ]);
    routes['/v1/chat/conversations/cnv_1/messages'] = messagesJson([
      for (var n = 1; n <= 40; n++)
        userMessage(seq: n, id: 'm$n', body: numbered[n - 1]),
    ]);
    await pumpChat(tester, routes: routes);

    expect(find.text(numbered.last), findsOneWidget);
    expect(find.text(numbered.first), findsNothing);
  });

  testWidgets('the «Revisar borrador» card keeps the bubbles margin', (
    tester,
  ) async {
    final routes = conversationRoutes();
    routes['/v1/chat/conversations/cnv_1/timeline'] = timelineJson([
      {
        'seq': 3,
        'kind': 'mail_draft',
        'account': 'trabajo',
        'draft_id': 'dft_1',
        'to': ['bea@ejemplo.org'],
        'subject': 'Presupuesto',
        'preview': 'Hola Bea',
        'created_at': '2026-10-22T10:00:00.000Z',
      },
    ]);
    routes['/v1/chat/conversations/cnv_1/messages'] = messagesJson([]);
    await pumpChat(tester, routes: routes);

    final card = tester.getRect(
      find.ancestor(
        of: find.text('Alfred preparó un borrador'),
        matching: find.byType(SurfaceCard),
      ),
    );
    final width = tester.view.physicalSize.width / tester.view.devicePixelRatio;
    expect(card.left, Spacing.lg);
    expect(card.right, width - Spacing.lg);
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
            body: HistoryDrawer(
              controller: controller,
              onOpenTeam: () {},
              onUnpaired: () async {},
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Historial'), findsOneWidget);
      expect(find.text('Plan semanal'), findsOneWidget);
      await revealHistoryChip(tester, 'Secciones');
      expect(find.text('Secciones'), findsOneWidget);

      await revealHistoryChip(tester, 'Fijados');
      await tester.tap(find.text('Fijados'));
      await tester.pumpAndSettle();
      expect(
        transport.calls.any((call) => call.contains('pinned=true')),
        isTrue,
      );
    },
  );

  testWidgets('only the newest own message shows every step of its turn', (
    tester,
  ) async {
    final routes = conversationRoutes();
    routes['/v1/chat/conversations/cnv_1/timeline'] = timelineJson([
      {
        ...timelineMessage(seq: 1, id: 'm1', preview: 'Crea el plan'),
        'turn_state': 'failed',
      },
      timelineMessage(
        seq: 2,
        id: 'm2',
        preview: 'Alfred: listo',
        author: 'agent',
      ),
      {
        ...timelineMessage(seq: 3, id: 'm3', preview: 'Ahora el resto'),
        'turn_state': 'running',
      },
    ]);
    routes['/v1/chat/conversations/cnv_1/messages'] = messagesJson([
      userMessage(seq: 1, id: 'm1', body: 'Crea el plan'),
      agentMessage(seq: 2, id: 'm2', body: 'Alfred: listo'),
      userMessage(seq: 3, id: 'm3', body: 'Ahora el resto'),
    ]);
    await pumpChat(tester, routes: routes);

    expect(find.text('Falló'), findsOneWidget);
    expect(find.text('Alfred lo está atendiendo'), findsOneWidget);
  });

  testWidgets('a message without a turn shows no step line', (tester) async {
    await pumpChat(tester, routes: conversationRoutes());

    expect(find.text('Enviado'), findsNothing);
    expect(find.text('Respondido'), findsNothing);
    expect(find.text('Falló'), findsNothing);
  });

  testWidgets('an open turn shows «trabajando…» in the header', (tester) async {
    final routes = conversationRoutes();
    routes['/v1/chat/conversations'] = conversationsJson(
      conversations: [
        {
          'id': 'cnv_1',
          'title': 'Plan semanal',
          'pinned': false,
          'archived': false,
          'last_seq': 2,
          'last_read_seq': 0,
          'unread': 0,
          'active_runs': 1,
          'last_message_at': '2026-10-22T10:00:00.000Z',
          'created_at': '2026-10-22T10:00:00.000Z',
          'updated_at': '2026-10-22T10:00:00.000Z',
        },
      ],
    );
    await pumpChat(tester, routes: routes);

    expect(find.text('trabajando…'), findsOneWidget);
  });

  testWidgets('an open turn shows its step in the header and the history row', (
    tester,
  ) async {
    await pumpChat(tester, routes: stepRoutes('Revisando los borradores'));

    expect(find.text('Alfred · Revisando los borradores'), findsOneWidget);
    await tester.tap(find.byTooltip('Abrir historial'));
    await tester.pumpAndSettle();
    expect(
      find.descendant(
        of: find.byType(HistoryDrawer),
        matching: find.text('Alfred · Revisando los borradores'),
      ),
      findsOneWidget,
    );
  });

  testWidgets('a step is plain text and at most 120 characters', (
    tester,
  ) async {
    const raw = '<b>Hola</b> [x](https://e.x) ';
    await pumpChat(tester, routes: stepRoutes('$raw${'x' * 200}'));

    final shown = '$raw${'x' * 200}'.substring(0, 120);
    expect(find.text('Alfred · $shown'), findsOneWidget);
  });

  testWidgets('a finished turn shows no step, whatever the list still says', (
    tester,
  ) async {
    final routes = stepRoutes('Revisando los borradores');
    routes['/v1/chat/conversations/cnv_1/timeline'] = timelineJson([
      {
        ...timelineMessage(seq: 1, id: 'm1', preview: 'Crea el plan'),
        'turn_state': 'completed',
      },
      timelineMessage(
        seq: 2,
        id: 'm2',
        preview: 'Alfred: listo',
        author: 'agent',
      ),
    ]);
    routes['/v1/chat/conversations/cnv_1/messages'] = messagesJson([
      userMessage(seq: 1, id: 'm1', body: 'Crea el plan'),
      agentMessage(seq: 2, id: 'm2', body: 'Alfred: listo'),
    ]);
    await pumpChat(tester, routes: routes);

    expect(find.textContaining('Revisando'), findsNothing);
    expect(find.text('trabajando…'), findsNothing);
  });

  testWidgets('the step sits under the newest own message while it runs', (
    tester,
  ) async {
    await pumpChat(tester, routes: stepRoutes('Redactando la respuesta'));

    expect(find.text('Alfred lo está atendiendo'), findsOneWidget);
    expect(find.text('Redactando la respuesta'), findsOneWidget);
  });

  testWidgets('«Ir al final» counts Alfred’s new messages and jumps back', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    List<Map<String, Object?>> timeline(int count) => [
      for (var n = 1; n <= count; n++)
        timelineMessage(
          seq: n,
          id: 'm$n',
          preview: 'Mensaje número $n',
          author: n.isEven ? 'agent' : 'user',
        ),
    ];
    List<Map<String, Object?>> messages(int count) => [
      for (var n = 1; n <= count; n++)
        n.isEven
            ? agentMessage(seq: n, id: 'm$n', body: 'Mensaje número $n')
            : userMessage(seq: n, id: 'm$n', body: 'Mensaje número $n'),
    ];
    final routes = conversationRoutes();
    routes['/v1/chat/conversations/cnv_1/timeline'] = timelineJson(
      timeline(40),
    );
    routes['/v1/chat/conversations/cnv_1/messages'] = messagesJson(
      messages(40),
    );
    final controller = await pumpChat(tester, routes: routes);

    expect(find.bySemanticsLabel(RegExp('Ir al final')), findsNothing);

    await tester.drag(find.byType(ConversationView), const Offset(0, 400));
    await tester.pumpAndSettle();
    expect(
      find.bySemanticsLabel('Ir al final, 0 mensajes nuevos'),
      findsOneWidget,
    );

    routes['/v1/chat/conversations/cnv_1/timeline'] = timelineJson(
      timeline(42),
    );
    routes['/v1/chat/conversations/cnv_1/messages'] = messagesJson(
      messages(42),
    );
    await controller.refresh();
    await tester.pumpAndSettle();
    expect(
      find.bySemanticsLabel('Ir al final, 1 mensaje nuevo'),
      findsOneWidget,
    );

    await tester.tap(find.bySemanticsLabel('Ir al final, 1 mensaje nuevo'));
    await tester.pumpAndSettle();
    expect(find.bySemanticsLabel(RegExp('Ir al final')), findsNothing);
    semantics.dispose();
  });

  testWidgets('an open turn turns the send button into «Detener»', (
    tester,
  ) async {
    await pumpChat(tester, routes: runningRoutes());

    expect(find.byTooltip('Detener'), findsOneWidget);
    expect(find.byIcon(Icons.stop), findsOneWidget);
    expect(find.byTooltip('Enviar'), findsNothing);
  });

  testWidgets(
    'without a turn an empty composer offers the mic, and text brings send',
    (tester) async {
      await pumpChat(tester, routes: conversationRoutes());

      expect(find.byIcon(Icons.mic), findsOneWidget);
      expect(find.byTooltip('Enviar'), findsNothing);
      expect(find.byTooltip('Detener'), findsNothing);

      await tester.enterText(find.byType(TextField), 'hola');
      await tester.pump();
      expect(find.byIcon(Icons.mic), findsNothing);
      expect(find.byTooltip('Enviar'), findsOneWidget);
    },
  );

  testWidgets('tapping «Detener» asks the host to stop the open turn', (
    tester,
  ) async {
    final routes = runningRoutes();
    final transport = FakeChatTransport(routes)
      ..routes['POST /v1/chat/conversations/cnv_1/stop'] = jsonEncode({
        'ok': true,
        'stopped': true,
        'runs': 1,
      });
    await pumpChat(tester, routes: routes, transport: transport);

    await tester.tap(find.byTooltip('Detener'));
    await tester.pumpAndSettle();
    expect(transport.calls, contains('POST /v1/chat/conversations/cnv_1/stop'));
  });

  testWidgets(
    'a device without the respond grant cannot stop and is told why',
    (tester) async {
      final routes = runningRoutes();
      final transport = FakeChatTransport(routes);
      await pumpChat(tester, routes: routes, transport: transport);

      transport.failure = const HostError(403, '{"detail":"forbidden"}');
      await tester.tap(find.byTooltip('Detener'));
      await tester.pumpAndSettle();

      expect(
        find.textContaining(
          'No tienes permiso para detener esto desde este dispositivo.',
        ),
        findsWidgets,
      );
      final stop = tester.widget<IconButton>(
        find.widgetWithIcon(IconButton, Icons.stop),
      );
      expect(stop.onPressed, isNull);
    },
  );

  testWidgets('the stop button is labelled for assistive tech', (tester) async {
    final semantics = tester.ensureSemantics();
    await pumpChat(tester, routes: runningRoutes());

    expect(find.bySemanticsLabel('Detener'), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('a draft stays editable while the turn runs', (tester) async {
    await pumpChat(tester, routes: runningRoutes());

    await tester.enterText(find.byType(TextField), 'Un cambio más');
    final field = tester.widget<TextField>(find.byType(TextField));
    expect(field.enabled, isTrue);
    expect(field.controller?.text, 'Un cambio más');
  });
}
