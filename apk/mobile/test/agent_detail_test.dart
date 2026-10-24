import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/core/state/live_triage.dart';
import 'package:herdr_notify/src/rust/api/state.dart';
import 'package:herdr_notify/ui/agent_detail_screen.dart';
import 'package:herdr_notify/ui/theme.dart';

AgentEntry agent({
  required AgentStatus status,
  bool restored = false,
  String? cwd = '/repo',
}) => AgentEntry(
  paneId: 'w1:p1',
  workspaceId: 'w1',
  project: 'herdr-notify',
  agent: 'opencode',
  status: status,
  attention: 3,
  title: 'Add the pairing screen',
  cwd: cwd,
  focused: false,
  canResume: false,
  restored: restored,
);

LiveTriage triageWith(AgentStatus status, {bool restored = false}) {
  final triage = LiveTriage(
    GatewayClient(
      baseUrl: 'http://100.64.0.1:8788',
      deviceId: 'dev-x',
      privateKeyBase64: 'placeholder',
    ),
  );
  triage.reconciler.acceptState(
    StateSnapshot(
      revision: 3,
      counts: const <String, int>{},
      agents: <AgentEntry>[agent(status: status, restored: restored)],
    ),
  );
  return triage;
}

Future<void> pumpDetail(WidgetTester tester, LiveTriage triage) async {
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      home: AgentDetailScreen(triage: triage, paneId: 'w1:p1'),
    ),
  );
  await tester.pump();
}

void main() {
  testWidgets('a blocked agent gets the composer, the keys and stop', (
    tester,
  ) async {
    await pumpDetail(tester, triageWith(AgentStatus.blocked));

    expect(find.text('Responder'), findsOneWidget);
    expect(find.text('Aprobar (y)'), findsOneWidget);
    expect(find.text('Rechazar (n)'), findsOneWidget);
    expect(find.text('Detener agente'), findsOneWidget);
    final field = tester.widget<TextField>(find.byType(TextField));
    expect(field.enabled, isTrue);
    final send = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, 'Enviar'),
    );
    expect(send.onPressed, isNotNull);
  });

  testWidgets('stop asks for confirmation before closing the agent', (
    tester,
  ) async {
    await pumpDetail(tester, triageWith(AgentStatus.blocked));

    await tester.tap(find.text('Detener agente'));
    await tester.pumpAndSettle();

    expect(find.byType(AlertDialog), findsOneWidget);
    expect(find.textContaining('Se cerrará el proceso'), findsOneWidget);
    expect(find.text('Cancelar'), findsOneWidget);

    await tester.tap(find.text('Cancelar'));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsNothing);
  });

  testWidgets('an agent that is not blocked cannot be driven', (tester) async {
    await pumpDetail(tester, triageWith(AgentStatus.idle));

    expect(
      find.textContaining('Disponible cuando el agente está bloqueado'),
      findsOneWidget,
    );
    final field = tester.widget<TextField>(find.byType(TextField));
    expect(field.enabled, isFalse);
    final send = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, 'Enviar'),
    );
    expect(send.onPressed, isNull);
  });

  testWidgets('a restored pane hides stop and offers a new conversation', (
    tester,
  ) async {
    await pumpDetail(tester, triageWith(AgentStatus.unknown, restored: true));

    expect(find.textContaining('El agente terminó'), findsWidgets);
    expect(find.text('Detener agente'), findsNothing);
    expect(find.text('Nueva conversación aquí'), findsOneWidget);
    final field = tester.widget<TextField>(find.byType(TextField));
    expect(field.enabled, isFalse);
  });
}
