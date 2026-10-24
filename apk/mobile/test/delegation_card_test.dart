import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';
import 'package:herdr_notify/src/rust/api/error.dart' show FailureKind;
import 'package:herdr_notify/src/rust/api/error/chat.dart';
import 'package:herdr_notify/ui/chat/chat_bubbles.dart';
import 'package:herdr_notify/ui/chat/delegation_card.dart';
import 'package:herdr_notify/ui/theme.dart';

ChatEntry cardEntry({required String status, int attention = 0}) => ChatEntry(
  seq: 3,
  kind: 'card',
  cardId: 'act_dlg_1',
  delegationId: 'dlg_1',
  paneId: 'w1:p3',
  attention: attention,
  body: '',
  preview: 'Ernesto planifica',
  state: status,
  title: 'Alfred pidió a Ernesto',
  summary: 'Ernesto prepara el plan de tres hitos',
  status: status,
  createdAt: '2026-10-22T10:00:00.000Z',
  local: false,
);

Widget wrap(Widget child) => MaterialApp(
  theme: buildTheme(),
  home: Scaffold(body: SingleChildScrollView(child: child)),
);

void main() {
  test('host titles in Latin-1 mojibake are repaired without touching clean text',
      () {
    expect(normalizeCardText('Ernesto terminÃ³'), 'Ernesto terminó');
    expect(normalizeCardText('Ernesto terminó'), 'Ernesto terminó');
    expect(normalizeCardText('Ana terminÃ³ la parte'), 'Ana terminó la parte');
  });

  testWidgets('every delegation state paints its label and live affordances', (
    tester,
  ) async {
    const states = {
      'queued': 'En cola',
      'running': 'En curso',
      'waiting': 'Esperando',
      'done': 'Completada',
      'completed': 'Completada',
      'failed': 'Falló',
      'cancelled': 'Cancelada',
      'interrupted': 'Interrumpida',
    };
    for (final entry in states.entries) {
      await tester.pumpWidget(
        wrap(
          DelegationCard(
            entry: cardEntry(status: entry.key),
            onOpenAgent: (_) {},
            onOpenDetail: (_) {},
            onCancel: (_) {},
          ),
        ),
      );

      expect(find.text('Alfred pidió a Ernesto'), findsOneWidget);
      expect(find.text(entry.value), findsOneWidget);
      expect(
        find.text('Ernesto prepara el plan de tres hitos'),
        findsOneWidget,
      );
      expect(find.text('Abrir agente'), findsOneWidget);
      expect(find.text('Ver resultado completo'), findsOneWidget);
      final active = delegationIsActive(entry.key);
      expect(find.text('Cancelar'), active ? findsOneWidget : findsNothing);
      expect(
        find.byType(LinearProgressIndicator),
        active ? findsOneWidget : findsNothing,
      );
    }
  });

  testWidgets('the card raises the three callbacks with its entry', (
    tester,
  ) async {
    ChatEntry? agent;
    ChatEntry? detail;
    ChatEntry? cancel;
    await tester.pumpWidget(
      wrap(
        DelegationCard(
          entry: cardEntry(status: 'running'),
          onOpenAgent: (entry) => agent = entry,
          onOpenDetail: (entry) => detail = entry,
          onCancel: (entry) => cancel = entry,
        ),
      ),
    );

    await tester.tap(find.text('Abrir agente'));
    await tester.tap(find.text('Ver resultado completo'));
    await tester.tap(find.text('Cancelar'));
    expect(agent?.delegationId, 'dlg_1');
    expect(detail?.delegationId, 'dlg_1');
    expect(cancel?.delegationId, 'dlg_1');
  });

  testWidgets('cancelling asks first and only then reports confirmation', (
    tester,
  ) async {
    bool? answer;
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () async {
                answer = await confirmDelegationCancel(
                  context,
                  title: 'Alfred pidió a Ernesto',
                );
              },
              child: const Text('Abrir diálogo'),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('Abrir diálogo'));
    await tester.pumpAndSettle();
    expect(find.text('¿Cancelar la delegación?'), findsOneWidget);
    expect(find.textContaining('historial se conservan'), findsOneWidget);
    await tester.tap(find.text('Volver'));
    await tester.pumpAndSettle();
    expect(answer, isFalse);

    await tester.tap(find.text('Abrir diálogo'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Cancelar delegación'));
    await tester.pumpAndSettle();
    expect(answer, isTrue);
  });

  testWidgets('each typed refusal renders a clear card', (tester) async {
    const failures = [
      ChatFailure(
        code: 'depth_exceeded',
        shownInTimeline: true,
        keptByHost: false,
        message: 'La delegación supera la profundidad permitida.',
        action: 'Alfred delega en el lead y el lead en un especialista.',
        kind: FailureKind.retry,
      ),
      ChatFailure(
        code: 'pane_limit',
        shownInTimeline: true,
        keptByHost: false,
        message: 'El host ya tiene seis paneles vivos.',
        action: 'Detén un agente en Equipo y vuelve a intentarlo.',
        kind: FailureKind.retry,
      ),
      ChatFailure(
        code: 'cycle_detected',
        shownInTimeline: true,
        keptByHost: false,
        message: 'Esa delegación repetiría un ciclo.',
        action: 'Cambia el destinatario o la tarea.',
        kind: FailureKind.retry,
      ),
      ChatFailure(
        code: 'turn_budget',
        shownInTimeline: true,
        keptByHost: false,
        message: 'Este turno ya hizo doce delegaciones.',
        action: 'Espera al siguiente turno.',
        kind: FailureKind.retry,
      ),
      ChatFailure(
        code: 'hour_budget',
        shownInTimeline: true,
        keptByHost: false,
        message: 'Se alcanzaron sesenta delegaciones en la última hora.',
        action: 'Espera antes de delegar más trabajo.',
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
        action: 'Reintenta cuando termine el turno actual.',
        kind: FailureKind.retry,
      ),
      ChatFailure(
        code: 'forbidden',
        shownInTimeline: true,
        keptByHost: false,
        message: 'El dispositivo no tiene el permiso RESPOND.',
        action: 'herdr pair --permission respond',
        kind: FailureKind.permission,
      ),
      ChatFailure(
        code: 'delegation_cancelled',
        shownInTimeline: true,
        keptByHost: false,
        message: 'La delegación ya terminó o fue cancelada.',
        action: 'Actualiza la tarjeta.',
        kind: FailureKind.retry,
      ),
      ChatFailure(
        code: 'no_such_delegation',
        shownInTimeline: true,
        keptByHost: false,
        message: 'La delegación ya no existe en el equipo.',
        action: 'Actualiza el timeline.',
        kind: FailureKind.retry,
      ),
    ];
    for (final failure in failures) {
      await tester.pumpWidget(
        wrap(
          ChatFailureCard(failure: failure, onRetry: () {}, onDismiss: () {}),
        ),
      );
      expect(find.text(failure.message), findsOneWidget);
      expect(find.text(failure.action!), findsOneWidget);
      if (failure.code == 'forbidden') {
        expect(find.text('Copiar comando'), findsOneWidget);
      } else {
        expect(find.text('Reintentar'), findsOneWidget);
      }
    }
  });
}
