import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/recipes_api.dart';
import 'package:herdr_notify/core/state/chat_controller.dart';
import 'package:herdr_notify/core/state/hires_controller.dart';
import 'package:herdr_notify/core/state/recipes_controller.dart';
import 'package:herdr_notify/src/rust/api/state.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/chat/chat_screen.dart';
import 'package:herdr_notify/ui/team/hires_section.dart';
import 'package:herdr_notify/ui/theme.dart';
import 'package:herdr_notify/ui/widgets/agent_row.dart';
import 'package:herdr_notify/ui/widgets/empty_state.dart';

import 'support/agent_fixtures.dart';
import 'support/captures.dart';
import 'support/chat_fixtures.dart';
import 'support/fake_chat_transport.dart';
import 'support/fake_team_host.dart';
import 'support/widget_mount.dart';

/// Set to a folder to save the approved palettes' captures; unset, they skip.
final String? _evidence = evidenceDir('APK_REDISENO_EVIDENCE');

void main() {
  setUpAll(() async {
    await RustLib.init();
    if (_evidence != null) await loadReadableFonts();
  });

  group('capturas', () {
    for (final palette in [HerdrPalettes.noche, HerdrPalettes.papel]) {
      testWidgets('Chat en ${palette.name}', skip: _evidence == null, (
        tester,
      ) async {
        final transport = FakeChatTransport(emptyRoutes());
        final controller = ChatController(transport, dummyTriage());
        await controller.start();
        addTearDown(controller.dispose);
        final key = await _pumpCapture(
          tester,
          palette,
          ChatScreen(
            controller: controller,
            hires: HiresController(transport: transport),
            onOpenTeam: () {},
            onUnpaired: () async {},
          ),
        );
        await saveCapture(tester, key, _evidence, 'redis-chat-${palette.id}');
      });

      testWidgets(
        'Chat con equipo en ${palette.name}',
        skip: _evidence == null,
        (tester) async {
          final transport = FakeChatTransport(emptyRoutes());
          final controller = ChatController(transport, dummyTriage());
          await controller.start();
          addTearDown(controller.dispose);
          final hires = HiresController(
            transport: FakeTeamHost(hires: [bobHire()]),
          );
          await hires.load();
          final key = await _pumpCapture(
            tester,
            palette,
            ChatScreen(
              controller: controller,
              hires: hires,
              onOpenTeam: () {},
              onUnpaired: () async {},
            ),
          );
          await saveCapture(
            tester,
            key,
            _evidence,
            'redis-chat-equipo-${palette.id}',
          );
        },
      );
      testWidgets('Equipo en ${palette.name}', skip: _evidence == null, (
        tester,
      ) async {
        final hires = HiresController(
          transport: FakeTeamHost(hires: [bobHire()]),
        );
        await hires.load();
        addTearDown(hires.dispose);
        final key = await _pumpCapture(
          tester,
          palette,
          Scaffold(
            body: ListView(
              children: [
                TeamHiresSection(
                  hires: hires,
                  recipes: RecipesController(
                    api: RecipesApi(FakeTeamHost()),
                    conversationId: () => null,
                  ),
                ),
              ],
            ),
          ),
        );
        await saveCapture(tester, key, _evidence, 'redis-equipo-${palette.id}');
      });
    }
  });

  group('Chat', () {
    for (final scale in [1.3, 2.0]) {
      testWidgets('sin desbordes con la fuente al ${scale}x', (tester) async {
        final transport = FakeChatTransport(emptyRoutes());
        final controller = ChatController(transport, dummyTriage());
        await controller.start();
        addTearDown(controller.dispose);

        await mountAtPhone(
          tester,
          ChatScreen(
            controller: controller,
            hires: HiresController(transport: transport),
            onOpenTeam: () {},
            onUnpaired: () async {},
          ),
          textScale: scale,
        );

        expect(find.text('Mensaje a Alfred'), findsOneWidget);
        expect(tester.takeException(), isNull);
      });
    }
  });

  group('Equipo', () {
    for (final scale in [1.3, 2.0]) {
      testWidgets('sin desbordes con la fuente al ${scale}x', (tester) async {
        final hires = HiresController(
          transport: FakeTeamHost(hires: [bobHire()]),
        );
        await hires.load();
        addTearDown(hires.dispose);

        await mountAtPhone(
          tester,
          Scaffold(
            body: ListView(
              children: [
                TeamHiresSection(
                  hires: hires,
                  recipes: RecipesController(
                    api: RecipesApi(FakeTeamHost()),
                    conversationId: () => null,
                  ),
                ),
                EmptyState(
                  icon: Icons.inbox_outlined,
                  title: 'No hay agentes',
                  detail: 'Cuando un agente se ejecute aparecerá aquí.',
                  actionLabel: 'Nueva conversación',
                  onAction: () {},
                ),
              ],
            ),
          ),
          textScale: scale,
        );

        expect(find.text('Bob'), findsOneWidget);
        expect(tester.takeException(), isNull);
      });
    }

    testWidgets('una lista vacía tiene una sola acción, de texto', (
      tester,
    ) async {
      await mountWidget(
        tester,
        Scaffold(
          body: EmptyState(
            icon: Icons.inbox_outlined,
            title: 'No hay agentes',
            detail: 'Cuando un agente se ejecute aparecerá aquí.',
            actionLabel: 'Nueva conversación',
            onAction: () {},
          ),
        ),
      );

      expect(find.byType(FilledButton), findsNothing);
      expect(
        find.widgetWithText(TextButton, 'Nueva conversación'),
        findsOneWidget,
      );
    });

    testWidgets('cada estado de agente se lee en palabras, no solo en color', (
      tester,
    ) async {
      for (final status in AgentStatus.values) {
        await mountWidget(
          tester,
          Scaffold(
            body: AgentRow(
              agent: agent(status: status),
              onTap: () {},
            ),
          ),
        );
        expect(
          find.text(HerdrColors.noche.statusLabel(status)),
          findsOneWidget,
          reason: '$status',
        );
      }
    });
  });
}

/// Mounts [home] in [palette] on a phone-sized view; the key is the capture's.
Future<GlobalKey> _pumpCapture(
  WidgetTester tester,
  HerdrPalette palette,
  Widget home,
) async {
  tester.view.physicalSize = const Size(390, 844);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  final key = GlobalKey();
  await tester.pumpWidget(
    MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: buildTheme(palette),
      home: RepaintBoundary(key: key, child: home),
    ),
  );
  await tester.pump();
  return key;
}
