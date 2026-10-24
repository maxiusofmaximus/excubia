import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/src/rust/api/approvals/error.dart';
import 'package:herdr_notify/src/rust/api/approvals/model.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/chat/approval_card.dart';
import 'package:herdr_notify/ui/theme.dart';

import 'support/approval_fixtures.dart';

ChatApproval _record({
  String status = 'pending',
  String? expiresAt,
  String? question,
}) => parseApprovalRecord(
  json: recordJson(
    approvalRow(status: status, expiresAt: expiresAt, question: question),
  ),
)!;

Widget _card(
  ChatApproval approval, {
  required Brightness brightness,
  ApprovalFailure? failure,
  ValueChanged<bool>? onDecide,
  VoidCallback? onRecheckGrant,
}) => MaterialApp(
  theme: buildTheme(brightness),
  home: Scaffold(
    body: Padding(
      padding: const EdgeInsets.all(16),
      child: ApprovalCard(
        approval: approval,
        agentName: 'Bob',
        projectName: 'Demo',
        busy: false,
        failure: failure,
        onDecide: onDecide ?? (_) {},
        onRecheckGrant: onRecheckGrant,
      ),
    ),
  ),
);

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  for (final brightness in [Brightness.light, Brightness.dark]) {
    group('${brightness.name} theme', () {
      testWidgets(
        'a pending card names the agent, the command, the level and the countdown',
        (tester) async {
          await tester.pumpWidget(_card(_record(), brightness: brightness));
          expect(find.text('Bob necesita permiso'), findsOneWidget);
          expect(find.text('Demo · demo/api'), findsOneWidget);
          expect(find.text('cargo test'), findsOneWidget);
          expect(find.text('riesgo medio'), findsOneWidget);
          expect(find.text('te lo pasa Alfred'), findsOneWidget);
          expect(find.textContaining('Caduca en'), findsOneWidget);
          expect(
            find.widgetWithText(FilledButton, 'Permitir una vez'),
            findsOneWidget,
          );
          expect(
            find.widgetWithText(OutlinedButton, 'Denegar'),
            findsOneWidget,
          );
          expect(find.textContaining('Permitir siempre'), findsNothing);
          expect(
            tester
                .getSize(find.widgetWithText(FilledButton, 'Permitir una vez'))
                .height,
            greaterThanOrEqualTo(48),
          );
          expect(
            tester
                .getSize(find.widgetWithText(OutlinedButton, 'Denegar'))
                .height,
            greaterThanOrEqualTo(48),
          );
          await tester.pumpWidget(const SizedBox());
        },
      );

      testWidgets('the two buttons answer once, with allow or deny', (
        tester,
      ) async {
        final answers = <bool>[];
        await tester.pumpWidget(
          _card(_record(), brightness: brightness, onDecide: answers.add),
        );
        await tester.tap(find.widgetWithText(FilledButton, 'Permitir una vez'));
        await tester.tap(find.widgetWithText(OutlinedButton, 'Denegar'));
        expect(answers, [true, false]);
        await tester.pumpWidget(const SizedBox());
      });

      testWidgets('a question replaces the command in plain text', (
        tester,
      ) async {
        await tester.pumpWidget(
          _card(
            _record(question: '¿Seguir con el deploy?'),
            brightness: brightness,
          ),
        );
        expect(find.text('¿Seguir con el deploy?'), findsOneWidget);
        expect(find.text('cargo test'), findsNothing);
        await tester.pumpWidget(const SizedBox());
      });

      testWidgets('a closed card drops its buttons and says how it ended', (
        tester,
      ) async {
        const closed = {
          'approved': 'Permitida una vez',
          'consumed': 'Permitida y ejecutada',
          'denied': 'Denegada',
          'expired': 'Caducada',
        };
        for (final entry in closed.entries) {
          await tester.pumpWidget(
            _card(_record(status: entry.key), brightness: brightness),
          );
          expect(find.text(entry.value), findsOneWidget, reason: entry.key);
          expect(
            find.widgetWithText(FilledButton, 'Permitir una vez'),
            findsNothing,
          );
          expect(find.widgetWithText(OutlinedButton, 'Denegar'), findsNothing);
        }
        await tester.pumpWidget(const SizedBox());
      });

      testWidgets('a pending card past the host deadline reads as expired', (
        tester,
      ) async {
        final past = DateTime.now()
            .toUtc()
            .subtract(const Duration(seconds: 5))
            .toIso8601String();
        await tester.pumpWidget(
          _card(_record(expiresAt: past), brightness: brightness),
        );
        expect(find.text('Caducada'), findsOneWidget);
        expect(
          find.widgetWithText(FilledButton, 'Permitir una vez'),
          findsNothing,
        );
        await tester.pumpWidget(const SizedBox());
      });

      testWidgets('a 403 shows the canonical host line and no button', (
        tester,
      ) async {
        await tester.pumpWidget(
          _card(
            _record(),
            brightness: brightness,
            failure: const ApprovalFailure(
              kind: ApprovalFailureKind.needsGrant,
              command: grantCommand,
              code: 'forbidden',
            ),
          ),
        );
        expect(
          find.text('Este dispositivo no puede aprobar. En el host, ejecuta:'),
          findsOneWidget,
        );
        expect(
          tester.widget<SelectableText>(find.byType(SelectableText)).data,
          grantCommand,
        );
        expect(
          find.widgetWithText(FilledButton, 'Permitir una vez'),
          findsNothing,
        );
        await tester.pumpWidget(const SizedBox());
      });

      testWidgets('a 409 already decided says so and offers no button', (
        tester,
      ) async {
        await tester.pumpWidget(
          _card(
            _record(),
            brightness: brightness,
            failure: const ApprovalFailure(
              kind: ApprovalFailureKind.alreadyDecided,
              code: 'approval_already_decided',
            ),
          ),
        );
        expect(find.text('Ya la decidió otro dispositivo.'), findsOneWidget);
        expect(
          find.widgetWithText(FilledButton, 'Permitir una vez'),
          findsNothing,
        );
        await tester.pumpWidget(const SizedBox());
      });
    });
  }

  testWidgets('the grant notice offers a recheck once the grant may exist', (
    tester,
  ) async {
    var rechecks = 0;
    await tester.pumpWidget(
      _card(
        _record(),
        brightness: Brightness.light,
        failure: const ApprovalFailure(
          kind: ApprovalFailureKind.needsGrant,
          command: grantCommand,
          code: 'forbidden',
        ),
        onRecheckGrant: () => rechecks++,
      ),
    );
    expect(find.text(grantCommand), findsOneWidget);
    await tester.tap(find.text('Ya lo he concedido'));
    expect(rechecks, 1);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('a card without detail says so instead of the generic summary', (
    tester,
  ) async {
    final row = Map<String, Object?>.from(approvalRow())
      ..remove('payload_redacted')
      ..['summary'] = 'command Bash';
    final approval = parseApprovalRecord(json: recordJson(row))!;
    await tester.pumpWidget(_card(approval, brightness: Brightness.light));
    expect(find.text('Falta el detalle del comando'), findsOneWidget);
    expect(find.text('command Bash'), findsNothing);
    await tester.pumpWidget(const SizedBox());
  });
}
