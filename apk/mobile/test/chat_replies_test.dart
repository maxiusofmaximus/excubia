import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/state/chat_controller.dart';
import 'package:herdr_notify/core/state/hires_controller.dart';
import 'package:herdr_notify/core/state/live_triage.dart';
import 'package:herdr_notify/src/rust/api/chat/reducer.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/chat/chat_screen.dart';
import 'package:herdr_notify/ui/theme.dart';

import 'support/fake_chat_transport.dart';

LiveTriage dummyTriage() => LiveTriage(
  HostClient(
    baseUrl: 'http://100.64.0.1:8788',
    deviceId: 'dev-x',
    privateKeyBase64: 'placeholder',
  ),
);

Map<String, Object?> agentWith(
  int seq,
  String id,
  String text,
  List<String> suggestions,
) => {
  ...timelineMessage(seq: seq, id: id, preview: text, author: 'agent'),
  'suggestions': suggestions,
};

List<Map<String, Object?>> standardTimeline() => [
  timelineMessage(seq: 1, id: 'm1', preview: 'Crea el plan'),
  agentWith(2, 'm2', '¿Qué nombre?', ['Sí, créalo', 'Cámbialo a Beta']),
];

List<Map<String, Object?>> standardMessages() => [
  userMessage(seq: 1, id: 'm1', body: 'Crea el plan'),
  agentMessage(seq: 2, id: 'm2', body: '¿Qué nombre?'),
];

Map<String, String> threadRoutes({
  required List<Map<String, Object?>> timeline,
  required List<Map<String, Object?>> messages,
}) => {
  '/v1/chat/bootstrap': bootstrapJson(),
  '/v1/chat/sections': sectionsJson(const []),
  '/v1/chat/conversations': conversationsJson(
    conversations: [
      {
        'id': 'cnv_1',
        'title': 'Plan semanal',
        'pinned': false,
        'archived': false,
        'last_seq': timeline.length,
        'last_read_seq': 0,
        'unread': 0,
        'last_message_at': '2026-10-22T10:00:00.000Z',
        'created_at': '2026-10-22T10:00:00.000Z',
        'updated_at': '2026-10-22T10:00:00.000Z',
      },
    ],
  ),
  '/v1/chat/conversations/cnv_1/timeline': timelineJson(timeline),
  '/v1/chat/conversations/cnv_1/messages': messagesJson(messages),
  '/v1/chat/conversations/cnv_1/read': readJson(timeline.length),
  'POST /v1/chat/conversations/cnv_1/messages': acceptedJson(
    seq: timeline.length + 1,
    id: 'm_new',
    body: 'Sí, créalo',
  ),
};

Future<void> pumpThread(
  WidgetTester tester,
  FakeChatTransport transport,
) async {
  final controller = ChatController(transport, dummyTriage());
  await controller.start();
  addTearDown(controller.dispose);
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      home: ChatScreen(
        controller: controller,
        hires: HiresController(transport: transport),
        onOpenTeam: () {},
        onUnpaired: () async {},
      ),
    ),
  );
  await tester.pumpAndSettle();
}

Map<String, Object?> lastPost(FakeChatTransport transport) {
  final index = transport.calls.lastIndexWhere(
    (call) => call == 'POST /v1/chat/conversations/cnv_1/messages',
  );
  return jsonDecode(utf8.decode(transport.bodies[index]))
      as Map<String, Object?>;
}

/// Long-press «Crea el plan», then «Responder»: the reply bar is up.
Future<void> startReplyToFirst(WidgetTester tester) async {
  await tester.longPress(find.text('Crea el plan'));
  await tester.pumpAndSettle();
  await tester.tap(find.text('Responder'));
  await tester.pumpAndSettle();
}

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  testWidgets('chips show only on the newest agent message', (tester) async {
    final transport = FakeChatTransport(
      threadRoutes(
        timeline: [
          agentWith(1, 'm1', '¿Qué nombre?', ['Vieja']),
          agentWith(2, 'm2', 'Listo, ¿seguro?', ['Nueva']),
        ],
        messages: [
          agentMessage(seq: 1, id: 'm1', body: '¿Qué nombre?'),
          agentMessage(seq: 2, id: 'm2', body: 'Listo, ¿seguro?'),
        ],
      ),
    );
    await pumpThread(tester, transport);

    expect(find.widgetWithText(OutlinedButton, 'Nueva'), findsOneWidget);
    expect(find.widgetWithText(OutlinedButton, 'Vieja'), findsNothing);
  });

  testWidgets('chips disappear once the operator has answered', (tester) async {
    final transport = FakeChatTransport(
      threadRoutes(
        timeline: [
          ...standardTimeline(),
          timelineMessage(seq: 3, id: 'm3', preview: 'Sí, créalo'),
        ],
        messages: [
          ...standardMessages(),
          userMessage(seq: 3, id: 'm3', body: 'Sí, créalo'),
        ],
      ),
    );
    await pumpThread(tester, transport);

    expect(find.widgetWithText(OutlinedButton, 'Sí, créalo'), findsNothing);
    expect(
      find.widgetWithText(OutlinedButton, 'Cámbialo a Beta'),
      findsNothing,
    );
  });

  testWidgets('a chip sends its exact text as a plain message', (tester) async {
    final transport = FakeChatTransport(
      threadRoutes(timeline: standardTimeline(), messages: standardMessages()),
    );
    await pumpThread(tester, transport);

    await tester.tap(find.widgetWithText(OutlinedButton, 'Sí, créalo'));
    await tester.pumpAndSettle();

    final body = lastPost(transport);
    expect(body['text'], 'Sí, créalo');
    expect(body.containsKey('reply_to'), isFalse);
    expect(
      find.widgetWithText(OutlinedButton, 'Cámbialo a Beta'),
      findsNothing,
    );
  });

  testWidgets('long-press, Responder, bar, send with reply_to, bar gone', (
    tester,
  ) async {
    final transport = FakeChatTransport(
      threadRoutes(timeline: standardTimeline(), messages: standardMessages()),
    );
    await pumpThread(tester, transport);

    await startReplyToFirst(tester);
    expect(find.text('Respondiendo a Tú: Crea el plan'), findsOneWidget);

    await tester.enterText(find.byType(TextField), 'Sí, créalo');
    await tester.pump();
    await tester.tap(find.byTooltip('Enviar'));
    await tester.pumpAndSettle();

    expect(lastPost(transport)['reply_to'], {'kind': 'message', 'id': 'm1'});
    expect(find.textContaining('Respondiendo a'), findsNothing);
  });

  testWidgets('the ✕ of the reply bar cancels the reply without sending', (
    tester,
  ) async {
    final transport = FakeChatTransport(
      threadRoutes(timeline: standardTimeline(), messages: standardMessages()),
    );
    await pumpThread(tester, transport);

    await startReplyToFirst(tester);
    await tester.tap(find.byTooltip('Cancelar respuesta'));
    await tester.pumpAndSettle();

    expect(find.textContaining('Respondiendo a'), findsNothing);
    expect(
      transport.calls.where(
        (call) => call == 'POST /v1/chat/conversations/cnv_1/messages',
      ),
      isEmpty,
    );
  });

  testWidgets('a quote is a button labelled with its author and preview', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    final transport = FakeChatTransport(
      threadRoutes(
        timeline: [
          ...standardTimeline(),
          {
            ...timelineMessage(seq: 3, id: 'm3', preview: 'Sí, créalo'),
            'quote': {
              'kind': 'message',
              'id': 'm2',
              'author_kind': 'agent',
              'preview': '¿Qué nombre?',
            },
          },
        ],
        messages: [
          ...standardMessages(),
          userMessage(seq: 3, id: 'm3', body: 'Sí, créalo'),
        ],
      ),
    );
    await pumpThread(tester, transport);

    expect(
      find.bySemanticsLabel('Responde a Alfred: ¿Qué nombre?'),
      findsOneWidget,
    );
    await tester.tap(find.bySemanticsLabel('Responde a Alfred: ¿Qué nombre?'));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    semantics.dispose();
  });

  testWidgets(
    'a refused reply keeps the text in the composer and drops the quote',
    (tester) async {
      final transport = FakeChatTransport(
        threadRoutes(
          timeline: standardTimeline(),
          messages: standardMessages(),
        ),
      );
      await pumpThread(tester, transport);

      await startReplyToFirst(tester);
      await tester.enterText(find.byType(TextField), 'Sí, créalo');
      await tester.pump();
      transport.failure = const HostError(
        422,
        '{"detail":{"code":"unknown_reply_to","field":"reply_to","message":"x"}}',
      );
      await tester.tap(find.byTooltip('Enviar'));
      await tester.pumpAndSettle();

      expect(
        tester.widget<TextField>(find.byType(TextField)).controller!.text,
        'Sí, créalo',
      );
      expect(find.textContaining('Respondiendo a'), findsNothing);
      expect(
        find.textContaining('ya no está en la conversación'),
        findsOneWidget,
      );
    },
  );

  test('a host without the reply fields decodes with no chips or quote', () {
    final reducer = newChatReducer();
    reducer.acceptTimeline(
      json:
          '{"ok":true,"entries":[{"seq":1,"kind":"message",'
          '"message_id":"m1","author_kind":"agent"}]}',
    );

    final entry = reducer.snapshot().entries.single;
    expect(entry.suggestions, isEmpty);
    expect(entry.quote, isNull);
  });
}
