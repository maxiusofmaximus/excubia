import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/restore_api.dart';
import 'package:excubia/core/state/live_triage.dart';
import 'package:excubia/src/rust/api/state.dart';
import 'package:excubia/ui/agent_detail_screen.dart';
import 'package:excubia/ui/theme.dart';

AgentEntry agent({
  required AgentStatus status,
  bool restored = false,
  String? cwd = '/repo',
}) => AgentEntry(
  paneId: 'w1:p1',
  workspaceId: 'w1',
  project: 'excubia',
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
    HostClient(
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
      home: AgentDetailScreen(
        triage: triage,
        paneId: 'w1:p1',
        restore: RestoreApi(
          HostClient(
            baseUrl: 'http://100.64.0.1:8788',
            deviceId: 'dev-x',
            privateKeyBase64: 'placeholder',
          ),
        ),
      ),
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

    // The portrait card sits above the actions, so stop is below the fold.
    await tester.dragUntilVisible(
      find.text('Detener agente'),
      find.byType(ListView),
      const Offset(0, -150),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Detener agente'));
    await tester.pumpAndSettle();

    expect(find.byType(AlertDialog), findsOneWidget);
    expect(
      find.textContaining('Se cerrará la conversación con'),
      findsOneWidget,
    );
    expect(find.textContaining('proceso'), findsNothing);
    expect(find.textContaining('w1:p1'), findsNothing);
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

  testWidgets('a restored pane has no respond panel, only a new conversation', (
    tester,
  ) async {
    await pumpDetail(tester, triageWith(AgentStatus.unknown, restored: true));

    expect(find.text('Responder'), findsNothing);
    expect(find.text('Enviar'), findsNothing);
    expect(find.text('Aprobar (y)'), findsNothing);
    expect(find.byType(TextField), findsNothing);
    expect(find.text('Detener agente'), findsNothing);
    await tester.scrollUntilVisible(
      find.text('Nueva conversación aquí'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    expect(find.text('Nueva conversación aquí'), findsOneWidget);
  });

  testWidgets(
    'a finished conversation says so, offers versions, and never says proceso',
    (tester) async {
      await pumpDetail(tester, triageWith(AgentStatus.unknown, restored: true));

      expect(find.text('Esta conversación terminó'), findsOneWidget);
      await tester.scrollUntilVisible(
        find.text('Ver versiones'),
        200,
        scrollable: find.byType(Scrollable).first,
      );
      expect(find.text('Ver versiones'), findsOneWidget);
      expect(find.textContaining('proceso'), findsNothing);
      expect(find.textContaining('Restaurado'), findsNothing);
      expect(find.text('Copias del turno'), findsNothing);
      expect(find.text('Ver copias'), findsNothing);
    },
  );

  testWidgets('a live agent that is not blocked keeps the respond panel', (
    tester,
  ) async {
    await pumpDetail(tester, triageWith(AgentStatus.working));

    expect(find.text('Responder'), findsOneWidget);
    expect(find.text('Enviar'), findsOneWidget);
    expect(find.text('Nueva conversación aquí'), findsNothing);
  });

  testWidgets('the technical fields stay folded until «Detalles» is opened', (
    tester,
  ) async {
    await pumpDetail(tester, triageWith(AgentStatus.working));

    // The row sits below the portrait card and the actions, so it is below the fold.
    await tester.scrollUntilVisible(
      find.text('Detalles'),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    expect(find.text('Workspace'), findsNothing);
    expect(find.text('Pane'), findsNothing);

    await tester.tap(find.text('Detalles'));
    await tester.pumpAndSettle();

    expect(find.text('Workspace'), findsOneWidget);
    expect(find.text('Pane'), findsOneWidget);
    expect(find.text('Revisión del estado'), findsOneWidget);
    expect(
      find.text(
        'Solo se escriben respuestas y teclas permitidas; no hay chat ni '
        'adjuntos en esta fase.',
      ),
      findsOneWidget,
    );
  });

  testWidgets('an unknown state does not read «Sin actividad reciente»', (
    tester,
  ) async {
    await pumpDetail(tester, triageWith(AgentStatus.unknown));

    // The app's own name is not a project: nothing repeats it under the name.
    expect(find.text('excubia'), findsNothing);
    expect(find.text('Sin actividad reciente'), findsNothing);
  });

  testWidgets('a known state still shows its word under the name', (
    tester,
  ) async {
    await pumpDetail(tester, triageWith(AgentStatus.idle));

    expect(find.text('inactivo'), findsOneWidget);
  });
}
