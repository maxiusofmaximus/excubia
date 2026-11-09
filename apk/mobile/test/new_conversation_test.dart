import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/state/live_triage.dart';
import 'package:herdr_notify/src/rust/api/catalog.dart';
import 'package:herdr_notify/src/rust/api/error.dart';
import 'package:herdr_notify/ui/new_conversation_screen.dart';
import 'package:herdr_notify/ui/theme.dart';

LiveTriage dummyTriage() => LiveTriage(
  HostClient(
    baseUrl: 'http://100.64.0.1:8788',
    deviceId: 'dev-x',
    privateKeyBase64: 'placeholder',
  ),
);

CatalogEntry entry(
  String name, {
  bool available = true,
  bool? runnable,
  bool fixable = false,
  String? problem,
  String? label,
  bool isDefault = false,
}) => CatalogEntry(
  name: name,
  available: available,
  runnable: runnable ?? available,
  fixable: fixable,
  problem: problem,
  label: label ?? name,
  isDefault: isDefault,
);

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
    'agents the PC cannot start are shown with the cause, not chosen',
    (tester) async {
      await pumpScreen(
        tester,
        loader: () async => (
          entries: [
            entry('fake', label: 'Agente falso'),
            entry(
              'claude',
              available: false,
              label: 'Claude Code',
              problem: 'No está instalado en este equipo.',
            ),
            entry(
              'codex',
              available: true,
              runnable: false,
              fixable: true,
              label: 'Codex',
              problem: 'Windows no puede ejecutarlo tal como está instalado; se puede arreglar.',
            ),
          ],
          error: null,
        ),
      );

      expect(find.text('Agente falso'), findsOneWidget);
      expect(find.text('Claude Code'), findsOneWidget);
      expect(find.text('No está instalado en este equipo.'), findsOneWidget);
      expect(find.text('Codex'), findsOneWidget);
      expect(find.text('Arreglar ahora'), findsOneWidget, reason: 'only Codex');
      expect(find.textContaining('no puede explorarla'), findsOneWidget);

      // The list builds lazily: scroll the create button into view first.
      final createFinder = find.widgetWithText(
        FilledButton,
        'Crear conversación',
      );
      await tester.dragUntilVisible(
        createFinder,
        find.byType(ListView),
        const Offset(0, -200),
      );
      FilledButton create() => tester.widget<FilledButton>(createFinder);
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
    expect(
      find.textContaining('Instala uno (Claude, Codex, Gemini u OpenCode)'),
      findsOneWidget,
    );
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

  testWidgets("the PC's default agent is the one preselected", (tester) async {
    await pumpScreen(
      tester,
      loader: () async => (
        entries: [
          entry('fake', label: 'Agente falso'),
          entry('codex', label: 'Codex', isDefault: true),
        ],
        error: null,
      ),
    );

    final group = tester.widget<RadioGroup<String>>(
      find.byType(RadioGroup<String>),
    );
    expect(group.groupValue, 'codex');
  });

  testWidgets('without a runnable default the first runnable agent is chosen', (
    tester,
  ) async {
    await pumpScreen(
      tester,
      loader: () async => (
        entries: [
          entry('claude', available: false, label: 'Claude Code'),
          entry('fake', label: 'Agente falso'),
          entry('codex', label: 'Codex', isDefault: true, runnable: false),
        ],
        error: null,
      ),
    );

    final group = tester.widget<RadioGroup<String>>(
      find.byType(RadioGroup<String>),
    );
    expect(group.groupValue, 'fake');
  });
}
