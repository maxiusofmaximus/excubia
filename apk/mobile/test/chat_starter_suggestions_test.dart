import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/state/chat_controller.dart';
import 'package:herdr_notify/core/state/hires_controller.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/chat/chat_screen.dart';
import 'package:herdr_notify/ui/theme.dart';

import 'support/chat_fixtures.dart';
import 'support/fake_chat_transport.dart';
import 'support/fake_team_host.dart';

/// One conversation that is open and has no messages yet: a new chat.
Map<String, String> newChatRoutes() => {
  '/v1/chat/bootstrap': bootstrapJson(),
  '/v1/chat/sections': sectionsJson(const []),
  '/v1/chat/conversations': conversationsJson(
    conversations: [
      {
        'id': 'cnv_1',
        'title': 'Nueva conversación',
        'pinned': false,
        'archived': false,
        'last_seq': 0,
        'last_read_seq': 0,
        'unread': 0,
        'last_message_at': '2026-11-05T10:00:00.000Z',
        'created_at': '2026-11-05T10:00:00.000Z',
        'updated_at': '2026-11-05T10:00:00.000Z',
      },
    ],
  ),
  '/v1/chat/conversations/cnv_1/timeline': timelineJson([]),
  '/v1/chat/conversations/cnv_1/messages': messagesJson([]),
  '/v1/chat/conversations/cnv_1/read': readJson(0),
  'POST /v1/chat/conversations/cnv_1/messages': acceptedJson(
    seq: 1,
    id: 'm1',
    body: 'Revisa el informe',
  ),
};

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  testWidgets(
    'a new empty conversation offers the starters, and sending the first message removes them',
    (tester) async {
      final transport = FakeChatTransport(newChatRoutes());
      final hires = HiresController(
        transport: FakeTeamHost(hires: [bobHire()]),
      );
      await hires.load();
      final controller = ChatController(transport, dummyTriage());
      addTearDown(controller.dispose);
      await controller.start();
      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(),
          home: ChatScreen(
            controller: controller,
            hires: hires,
            onOpenTeam: () {},
            onUnpaired: () async {},
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(controller.current?.id, 'cnv_1');
      expect(controller.entries, isEmpty);
      expect(find.text('¿Cómo va el equipo?'), findsOneWidget);
      expect(
        find.text('Revisa que todo funcione antes de entregar'),
        findsOneWidget,
      );
      expect(find.text('Crea el proyecto Demo'), findsNothing);

      await controller.send('Revisa el informe');
      await tester.pumpAndSettle();

      expect(controller.entries, isNotEmpty);
      expect(find.text('¿Cómo va el equipo?'), findsNothing);
    },
  );
}
