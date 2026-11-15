import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/state/approvals_controller.dart';
import 'package:excubia/core/state/chat_controller.dart';
import 'package:excubia/core/state/hires_controller.dart';
import 'package:excubia/src/rust/frb_generated.dart';
import 'package:excubia/ui/chat/approval_card.dart';
import 'package:excubia/ui/chat/chat_conversation_view.dart';
import 'package:excubia/ui/chat/chat_screen.dart';
import 'package:excubia/ui/theme.dart';

import 'support/approval_fixtures.dart';
import 'support/chat_fixtures.dart';
import 'support/fake_chat_transport.dart';

/// A conversation of 40 messages, so the timeline scrolls; the inbox is [inbox].
Map<String, String> _routes(String inbox) => {
  '/v1/chat/bootstrap': bootstrapJson(),
  '/v1/chat/sections': sectionsJson(const []),
  '/v1/chat/conversations': conversationsJson(
    conversations: [
      {
        'id': 'cnv_1',
        'title': 'Plan semanal',
        'pinned': false,
        'archived': false,
        'last_seq': 40,
        'last_read_seq': 40,
        'unread': 0,
        'last_message_at': '2026-10-22T10:00:00.000Z',
        'created_at': '2026-10-22T10:00:00.000Z',
        'updated_at': '2026-10-22T10:00:00.000Z',
      },
    ],
  ),
  '/v1/chat/conversations/cnv_1/timeline': timelineJson([
    for (var n = 1; n <= 40; n++)
      timelineMessage(
        seq: n,
        id: 'm$n',
        preview: 'Mensaje número $n',
        author: n.isEven ? 'agent' : 'user',
      ),
  ]),
  '/v1/chat/conversations/cnv_1/messages': messagesJson([
    for (var n = 1; n <= 40; n++)
      n.isEven
          ? agentMessage(seq: n, id: 'm$n', body: 'Mensaje número $n')
          : userMessage(seq: n, id: 'm$n', body: 'Mensaje número $n'),
  ]),
  '/v1/chat/conversations/cnv_1/read': readJson(40),
  'GET /v1/chat/approvals': inbox,
  'GET /v1/chat/approvals/settings': settingsJson(),
};

/// The chat with its approvals loaded before it opens, as the app does.
Future<ApprovalsController> _pump(
  WidgetTester tester,
  Map<String, String> routes,
) async {
  final transport = FakeChatTransport(routes);
  final approvals = ApprovalsController(
    transport: transport,
    deviceId: 'dev_1',
  );
  final chat = ChatController(transport, dummyTriage(), approvals: approvals);
  addTearDown(() {
    chat.dispose();
    approvals.dispose();
  });
  await chat.start();
  await approvals.refresh();
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      home: ChatScreen(
        controller: chat,
        hires: HiresController(transport: transport),
        onOpenTeam: () {},
        onUnpaired: () async {},
        pickDocuments: () async => const [],
      ),
    ),
  );
  await tester.pumpAndSettle();
  return approvals;
}

/// The timeline's own scroll position: the first scrollable under the view.
ScrollPosition _position(WidgetTester tester) => tester
    .state<ScrollableState>(
      find
          .descendant(
            of: find.byType(ConversationView),
            matching: find.byType(Scrollable),
          )
          .first,
    )
    .position;

Future<void> _scrollUp(WidgetTester tester) async {
  await tester.drag(find.byType(ConversationView), const Offset(0, 400));
  await tester.pumpAndSettle();
}

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  testWidgets('a new pending approval jumps to the end, once', (tester) async {
    final routes = _routes(inboxJson(const []));
    final approvals = await _pump(tester, routes);
    await _scrollUp(tester);
    final away = _position(tester);
    expect(away.maxScrollExtent - away.pixels, greaterThan(140));

    // The approval arrives while the user reads above: its card must come into view.
    routes['GET /v1/chat/approvals'] = inboxJson([
      approvalRow(command: 'cargo test'),
    ]);
    await approvals.refresh();
    await tester.pumpAndSettle();
    final atEnd = _position(tester);
    expect(atEnd.pixels, closeTo(atEnd.maxScrollExtent, 0.5));
    expect(find.byType(ApprovalCardHost), findsOneWidget);

    // The same approval on the next refresh does not drag the user back down.
    await _scrollUp(tester);
    final scrolledUp = _position(tester).pixels;
    await approvals.refresh();
    await tester.pumpAndSettle();
    expect(_position(tester).pixels, scrolledUp);
  });

  testWidgets('a mail card follows its facts down to its buttons', (
    tester,
  ) async {
    final routes = _routes(inboxJson(const []));
    final approvals = await _pump(tester, routes);

    // First the frame: a mail card without its facts yet.
    routes['GET /v1/chat/approvals'] = inboxJson([
      approvalRow(tool: 'mail.attach_path'),
    ]);
    await approvals.refresh();
    await tester.pumpAndSettle();
    await _scrollUp(tester);

    // Then the facts arrive and the card grows: the chat goes to its end again.
    routes['GET /v1/chat/approvals'] = inboxJson([
      approvalRow(
        tool: 'mail.attach_path',
        details: {
          'path': r'D:\Docs\informe.xlsx',
          'size': 2048,
          'sha256': 'ab',
        },
      ),
    ]);
    await approvals.refresh();
    await tester.pumpAndSettle();
    final atEnd = _position(tester);
    expect(atEnd.pixels, closeTo(atEnd.maxScrollExtent, 0.5));
    expect(find.text('Permitir una vez'), findsOneWidget);
  });

  testWidgets(
    '«Ir al final» counts the pending approvals of this conversation only',
    (tester) async {
      final semantics = tester.ensureSemantics();
      await _pump(
        tester,
        _routes(
          inboxJson([
            approvalRow(id: 'apr_1', command: 'cargo test'),
            approvalRow(id: 'apr_2', command: 'otra', conversationId: 'cnv_2'),
            approvalRow(id: 'apr_3', command: 'hecha', status: 'approved'),
            approvalRow(id: 'apr_4', command: 'dos'),
          ]),
        ),
      );
      // Opened with its approvals in view: nothing new to jump to, the button waits.
      expect(find.bySemanticsLabel(RegExp('Ir al final')), findsNothing);

      await _scrollUp(tester);
      expect(
        find.bySemanticsLabel('Ir al final, 2 aprobaciones pendientes'),
        findsOneWidget,
      );
      semantics.dispose();
    },
  );
}
