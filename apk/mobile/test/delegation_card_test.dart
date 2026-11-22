import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/src/rust/api/chat/model.dart';
import 'package:excubia/src/rust/api/error.dart' show FailureKind;
import 'package:excubia/src/rust/api/error/chat.dart';
import 'package:excubia/ui/chat/chat_bubbles.dart';
import 'package:excubia/ui/chat/delegation_card.dart';
import 'package:excubia/ui/chat/markdown_text.dart';
import 'package:excubia/ui/theme.dart';

ChatEntry cardEntry({
  required String status,
  int attention = 0,
  String title = 'Alfred pidió a Ernesto',
}) => ChatEntry(
  seq: 3,
  kind: 'card',
  suggestions: const [],
  cardId: 'act_dlg_1',
  delegationId: 'dlg_1',
  paneId: 'w1:p3',
  attention: attention,
  body: '',
  preview: 'Ernesto planifica',
  state: status,
  title: title,
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
  test(
    'host titles in Latin-1 mojibake are repaired without touching clean text',
    () {
      expect(normalizeCardText('Ernesto terminÃ³'), 'Ernesto terminó');
      expect(normalizeCardText('Ernesto terminó'), 'Ernesto terminó');
      expect(
        normalizeCardText('Ana terminÃ³ la parte'),
        'Ana terminó la parte',
      );
    },
  );

  testWidgets('every delegation state paints its label and live affordances', (
    tester,
  ) async {
    const lines = {
      'queued': 'Ernesto está trabajando en tu pedido',
      'running': 'Ernesto está trabajando en tu pedido',
      'waiting': 'Ernesto está esperando',
      'done': 'Ernesto terminó',
      'completed': 'Ernesto terminó',
      'failed': 'Ernesto falló',
      'cancelled': 'Delegación cancelada',
      'interrupted': 'Ernesto se interrumpió',
    };
    for (final entry in lines.entries) {
      await tester.pumpWidget(
        wrap(
          DelegationCard(
            entry: cardEntry(status: entry.key),
            onOpenDetail: (_) {},
          ),
        ),
      );

      expect(find.text(entry.value), findsOneWidget, reason: entry.key);
      expect(find.text('Ver detalle'), findsOneWidget);
      // The summary, the agent and the cancel button wait in the detail.
      expect(find.text('Ernesto prepara el plan de tres hitos'), findsNothing);
      expect(find.text('Abrir agente'), findsNothing);
      expect(find.text('Cancelar'), findsNothing);
      final active = delegationIsActive(entry.key);
      expect(
        find.byType(LinearProgressIndicator),
        active ? findsOneWidget : findsNothing,
      );
    }
  });

  testWidgets('the card opens the detail from its one button', (tester) async {
    ChatEntry? detail;
    await tester.pumpWidget(
      wrap(
        DelegationCard(
          entry: cardEntry(status: 'running'),
          onOpenDetail: (entry) => detail = entry,
        ),
      ),
    );

    await tester.tap(find.text('Ver detalle'));
    expect(detail?.delegationId, 'dlg_1');
  });

  testWidgets('a title in markdown reads plain and names the agent', (
    tester,
  ) async {
    await tester.pumpWidget(
      wrap(
        DelegationCard(
          entry: cardEntry(status: 'done', title: '**Alfred pidió a Ernesto**'),
          onOpenDetail: (_) {},
        ),
      ),
    );
    expect(find.text('Ernesto terminó'), findsOneWidget);
    expect(find.textContaining('**'), findsNothing);
  });

  test('the line names the agent and the attention it needs', () {
    expect(delegateName('Alfred pidió a Ernesto'), 'Ernesto');
    expect(delegateName('Ana asignó a Bob'), 'Bob');
    expect(delegateName('Delegación del equipo'), isNull);
    expect(
      delegationLine(null, 'running'),
      'Un agente está trabajando en tu pedido',
    );
    expect(
      delegationLine('Ernesto', 'running', attention: true),
      'Ernesto necesita tu atención',
    );
    expect(
      delegationLine('Ernesto', 'done', attention: true),
      'Ernesto terminó',
    );
  });

  test('markdown marks leave the card text plain', () {
    expect(
      plainFromMarkdown('**Resultado de la tarea:** `C:\\Users\\Max`'),
      'Resultado de la tarea: C:\\Users\\Max',
    );
    expect(plainFromMarkdown('Tarea *lista*'), 'Tarea lista');
    expect(plainFromMarkdown('Sin marcas'), 'Sin marcas');
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
