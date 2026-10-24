import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/core/state/live_triage.dart';
import 'package:herdr_notify/src/rust/api/catalog.dart';
import 'package:herdr_notify/src/rust/api/error.dart';
import 'package:herdr_notify/ui/new_conversation_screen.dart';
import 'package:herdr_notify/ui/theme.dart';

LiveTriage dummyTriage() => LiveTriage(
  GatewayClient(
    baseUrl: 'http://100.64.0.1:8788',
    deviceId: 'dev-x',
    privateKeyBase64: 'placeholder',
  ),
);

CatalogEntry entry(String name, {bool available = true, String? label}) =>
    CatalogEntry(name: name, available: available, label: label ?? name);

Future<void> pumpScreen(
  WidgetTester tester, {
  required CatalogLoader loader,
  String? initialCwd,
}) async {
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      home: NewConversationScreen(
        triage: dummyTriage(),
        initialCwd: initialCwd,
        catalogLoader: loader,
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets(
    'only available agents are offered and the folder enables create',
    (tester) async {
      await pumpScreen(
        tester,
        loader: () async => (
          entries: [
            entry('fake', label: 'Agente falso'),
            entry('claude', available: false, label: 'Claude Code'),
          ],
          error: null,
        ),
      );

      expect(find.text('Agente falso'), findsOneWidget);
      expect(find.text('Claude Code'), findsNothing);
      expect(find.textContaining('no puede explorarla'), findsOneWidget);

      FilledButton create() => tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, 'Crear conversación'),
      );
      expect(create().onPressed, isNull);

      await tester.enterText(find.byType(TextField).first, r'D:\work');
      await tester.pump();
      expect(create().onPressed, isNotNull);
    },
  );

  testWidgets('an empty catalogue explains the cause and keeps create off', (
    tester,
  ) async {
    await pumpScreen(
      tester,
      loader: () async => (entries: const <CatalogEntry>[], error: null),
    );

    expect(find.text('No hay agentes disponibles'), findsOneWidget);
    expect(find.textContaining('agents.toml'), findsOneWidget);
    final create = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, 'Crear conversación'),
    );
    expect(create.onPressed, isNull);
  });

  testWidgets('a catalogue error shows the Rust message and can be retried', (
    tester,
  ) async {
    await pumpScreen(
      tester,
      loader: () async => (
        entries: null,
        error: const ErrorInfo(
          message: 'Falta el permiso READ_STATE para leer el estado.',
          action: 'Concédelo en el host.',
          kind: FailureKind.permission,
        ),
      ),
    );

    expect(find.text('No se pudo leer el catálogo'), findsOneWidget);
    expect(find.textContaining('READ_STATE'), findsOneWidget);
    expect(find.text('Reintentar'), findsOneWidget);
  });

  testWidgets('a prefilled folder from a restored pane is reused', (
    tester,
  ) async {
    await pumpScreen(
      tester,
      initialCwd: r'D:\projects\demo',
      loader: () async =>
          (entries: [entry('fake', label: 'Agente falso')], error: null),
    );

    final fields = tester
        .widgetList<TextField>(find.byType(TextField))
        .toList();
    expect(fields.first.controller!.text, r'D:\projects\demo');
    final create = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, 'Crear conversación'),
    );
    expect(create.onPressed, isNotNull);
  });
}
