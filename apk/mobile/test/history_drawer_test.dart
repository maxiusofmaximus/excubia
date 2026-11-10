import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/state/chat_controller.dart';
import 'package:excubia/core/state/live_triage.dart';
import 'package:excubia/src/rust/frb_generated.dart';
import 'package:excubia/ui/chat/history_drawer.dart';
import 'package:excubia/ui/theme.dart';

import 'support/fake_chat_transport.dart';
import 'support/history_chips.dart';

LiveTriage _dummyTriage() => LiveTriage(
  HostClient(
    baseUrl: 'http://100.64.0.1:8788',
    deviceId: 'dev-x',
    privateKeyBase64: 'placeholder',
  ),
);

Map<String, String> _routes({
  int all = 0,
  int attention = 0,
  int working = 0,
}) => {
  '/v1/chat/bootstrap': bootstrapJson(),
  '/v1/chat/sections': sectionsJson(const []),
  '/v1/chat/conversations': conversationsJson(),
  '/v1/chat/conversations/counts': countsJson(
    all: all,
    attention: attention,
    working: working,
  ),
};

/// The drawer rebuilds on the controller as the screen above it does.
Future<ChatController> _pump(
  WidgetTester tester,
  FakeChatTransport transport,
) async {
  final controller = ChatController(transport, _dummyTriage());
  addTearDown(controller.dispose);
  await controller.start();
  await controller.loadHistory();
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      home: ListenableBuilder(
        listenable: controller,
        builder: (context, _) => Scaffold(
          body: HistoryDrawer(
            controller: controller,
            onOpenTeam: () {},
            onUnpaired: () async {},
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return controller;
}

Finder _chipOf(String label) =>
    find.ancestor(of: find.text(label), matching: find.byType(ChoiceChip));

/// The chip labels in row order, collected while the row is dragged to its end.
Future<List<String>> _chipLabels(WidgetTester tester) async {
  final seen = <String>[];
  for (var step = 0; step < 4; step++) {
    for (final chip in tester.widgetList<ChoiceChip>(find.byType(ChoiceChip))) {
      final label = ((chip.label as Row).children.first as Text).data!;
      if (!seen.contains(label)) seen.add(label);
    }
    await tester.drag(historyChipRow(), const Offset(-200, 0));
    await tester.pumpAndSettle();
  }
  return seen;
}

const _planRow = <String, Object?>{
  'id': 'cnv_1',
  'title': 'Plan semanal',
  'pinned': false,
  'archived': false,
  'last_seq': 1,
  'last_read_seq': 0,
  'unread': 0,
  'last_message_at': '2026-10-22T10:00:00.000Z',
  'created_at': '2026-10-22T10:00:00.000Z',
  'updated_at': '2026-10-22T10:00:00.000Z',
};

Map<String, String> _deleteRoutes() => _routes()
  ..['/v1/chat/conversations'] = conversationsJson(conversations: [_planRow])
  ..['DELETE /v1/chat/conversations/cnv_1'] = jsonEncode({'ok': true});

/// Opens the row's menu and picks «Borrar», which only asks; nothing is sent yet.
Future<void> _chooseDelete(WidgetTester tester) async {
  await tester.tap(find.byTooltip('Acciones de la conversación'));
  await tester.pumpAndSettle();
  await tester.tap(find.text('Borrar'));
  await tester.pumpAndSettle();
}

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  testWidgets('six chips in order, each count shown above zero, 99+ above 99', (
    tester,
  ) async {
    await _pump(tester, FakeChatTransport(_routes(all: 120, attention: 3)));

    expect(find.text('99+'), findsOneWidget);
    expect(find.text('3'), findsOneWidget);
    expect(find.text('0'), findsNothing);
    expect(await _chipLabels(tester), [
      'Todos',
      'Pendientes de ti',
      'Trabajando',
      'Fijados',
      'Secciones',
      'Archivados',
    ]);
  });

  testWidgets(
    'each chip reads its name and count, and is at least 48 dp tall',
    (tester) async {
      final semantics = tester.ensureSemantics();
      await _pump(
        tester,
        FakeChatTransport(_routes(all: 4, attention: 2, working: 1)),
      );

      expect(tester.getSize(_chipOf('Todos')).height, greaterThanOrEqualTo(48));
      expect(find.bySemanticsLabel('Todos, 4'), findsOneWidget);
      expect(find.bySemanticsLabel('Pendientes de ti, 2'), findsOneWidget);

      await revealHistoryChip(tester, 'Trabajando');
      expect(find.bySemanticsLabel('Trabajando, 1'), findsOneWidget);
      semantics.dispose();
    },
  );

  testWidgets('the selected chip is marked by its check, not only its colour', (
    tester,
  ) async {
    await _pump(tester, FakeChatTransport(_routes(working: 1)));
    expect(tester.widget<ChoiceChip>(_chipOf('Todos')).selected, isTrue);

    await revealHistoryChip(tester, 'Trabajando');
    await tester.tap(find.text('Trabajando'));
    await tester.pumpAndSettle();

    expect(tester.widget<ChoiceChip>(_chipOf('Trabajando')).selected, isTrue);
    expect(
      tester.widget<ChoiceChip>(_chipOf('Trabajando')).showCheckmark,
      isTrue,
    );
  });

  testWidgets('an empty filter says what is missing in its own words', (
    tester,
  ) async {
    await _pump(tester, FakeChatTransport(_routes()));

    await tester.tap(find.text('Pendientes de ti'));
    await tester.pumpAndSettle();
    expect(find.text('Nada pendiente de ti.'), findsOneWidget);

    await tester.tap(find.text('Todos'));
    await tester.pumpAndSettle();
    expect(find.text('No hay conversaciones en este filtro.'), findsOneWidget);

    await revealHistoryChip(tester, 'Trabajando');
    await tester.tap(find.text('Trabajando'));
    await tester.pumpAndSettle();
    expect(find.text('Ningún agente está trabajando ahora.'), findsOneWidget);
  });

  testWidgets('an old host shows the four chips it always had', (tester) async {
    await _pump(tester, OldHostChatTransport(_routes()));

    expect(await _chipLabels(tester), [
      'Todos',
      'Fijados',
      'Secciones',
      'Archivados',
    ]);
  });

  testWidgets(
    'the row menu offers Borrar with the trash icon, after Archivar',
    (tester) async {
      await _pump(tester, FakeChatTransport(_deleteRoutes()));
      await tester.tap(find.byTooltip('Acciones de la conversación'));
      await tester.pumpAndSettle();

      expect(find.byIcon(Icons.delete_outline), findsOneWidget);
      expect(
        tester.getTopLeft(find.text('Borrar')).dy,
        greaterThan(tester.getTopLeft(find.text('Archivar')).dy),
      );
    },
  );

  testWidgets(
    'cancelling the question keeps the conversation and sends nothing',
    (tester) async {
      final transport = FakeChatTransport(_deleteRoutes());
      await _pump(tester, transport);

      await _chooseDelete(tester);
      expect(find.text('¿Borrar esta conversación?'), findsOneWidget);
      await tester.tap(find.text('Cancelar'));
      await tester.pumpAndSettle();

      expect(
        transport.calls.where((call) => call.startsWith('DELETE')),
        isEmpty,
      );
      expect(find.text('Plan semanal'), findsOneWidget);
    },
  );

  testWidgets('confirming deletes the conversation and says so', (
    tester,
  ) async {
    final transport = FakeChatTransport(_deleteRoutes());
    await _pump(tester, transport);

    await _chooseDelete(tester);
    transport.routes['/v1/chat/conversations'] = conversationsJson();
    await tester.tap(find.widgetWithText(FilledButton, 'Borrar'));
    await tester.pumpAndSettle();

    expect(transport.calls, contains('DELETE /v1/chat/conversations/cnv_1'));
    expect(find.text('Plan semanal'), findsNothing);
    expect(find.text('Conversación borrada'), findsOneWidget);
  });
}
