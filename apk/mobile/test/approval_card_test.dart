import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/ui/widgets/comic/comic_button.dart';
import 'package:excubia/ui/widgets/comic/comic_slab.dart';
import 'package:excubia/ui/widgets/comic/comic_tag.dart';
import 'package:excubia/src/rust/api/approvals/error.dart';
import 'package:excubia/src/rust/api/approvals/model.dart';
import 'package:excubia/src/rust/frb_generated.dart';
import 'package:excubia/ui/chat/approval_card.dart';
import 'package:excubia/ui/theme.dart';

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
  theme: buildTheme(
    brightness == Brightness.dark
        ? HerdrPalettes.noche
        : HerdrPalettes.pergamino,
  ),
  home: Scaffold(
    body: Padding(
      padding: const EdgeInsets.all(16),
      child: ApprovalCard(
        approval: approval,
        agentName: 'Bob',
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

  testWidgets('at 360 dp «Permitir una vez» reads on one line', (tester) async {
    tester.view.physicalSize = const Size(360, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await tester.pumpWidget(_card(_record(), brightness: Brightness.dark));

    final label = tester.renderObject<RenderParagraph>(
      find.descendant(
        of: find.widgetWithText(ComicButton, 'Permitir una vez'),
        matching: find.text('Permitir una vez'),
      ),
    );
    expect(label.didExceedMaxLines, isFalse);
    await tester.pumpWidget(const SizedBox());
  });

  for (final brightness in [Brightness.light, Brightness.dark]) {
    group('${brightness.name} theme', () {
      testWidgets(
        'a pending card names the agent, the command, the level and the countdown',
        (tester) async {
          await tester.pumpWidget(_card(_record(), brightness: brightness));
          expect(find.text('Bob quiere ejecutar un comando'), findsOneWidget);
          expect(find.text('Carpeta: demo/api'), findsOneWidget);
          // The command waits under «Detalles técnicos», closed.
          expect(find.text('cargo test'), findsNothing);
          expect(find.text('riesgo medio'), findsNothing);
          expect(find.text('te lo pasa Alfred'), findsNothing);
          expect(find.textContaining('Caduca en'), findsOneWidget);
          expect(
            find.widgetWithText(ComicButton, 'Permitir una vez'),
            findsOneWidget,
          );
          expect(find.widgetWithText(ComicButton, 'Denegar'), findsOneWidget);
          expect(find.textContaining('Permitir siempre'), findsNothing);
          expect(
            tester
                .getSize(find.widgetWithText(ComicButton, 'Permitir una vez'))
                .height,
            greaterThanOrEqualTo(48),
          );
          expect(
            tester.getSize(find.widgetWithText(ComicButton, 'Denegar')).height,
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
        await tester.tap(find.widgetWithText(ComicButton, 'Permitir una vez'));
        await tester.tap(find.widgetWithText(ComicButton, 'Denegar'));
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
            find.widgetWithText(ComicButton, 'Permitir una vez'),
            findsNothing,
          );
          expect(find.widgetWithText(ComicButton, 'Denegar'), findsNothing);
        }
        await tester.pumpWidget(const SizedBox());
      });

      testWidgets('a card that waits for days shows no countdown', (
        tester,
      ) async {
        final later = DateTime.now()
            .toUtc()
            .add(const Duration(days: 30))
            .toIso8601String();
        await tester.pumpWidget(
          _card(_record(expiresAt: later), brightness: brightness),
        );
        expect(find.textContaining('Caduca en'), findsNothing);
        expect(
          find.widgetWithText(ComicButton, 'Permitir una vez'),
          findsOneWidget,
        );
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
          find.widgetWithText(ComicButton, 'Permitir una vez'),
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
          find.text('Este dispositivo no puede aprobar. En el PC, ejecuta:'),
          findsOneWidget,
        );
        expect(
          tester.widget<SelectableText>(find.byType(SelectableText)).data,
          grantCommand,
        );
        expect(
          find.widgetWithText(ComicButton, 'Permitir una vez'),
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
          find.widgetWithText(ComicButton, 'Permitir una vez'),
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

  testWidgets('a card without a command shows no command section at all', (
    tester,
  ) async {
    final row = Map<String, Object?>.from(approvalRow())
      ..remove('payload_redacted')
      ..['summary'] = 'command Bash';
    final approval = parseApprovalRecord(json: recordJson(row))!;
    await tester.pumpWidget(_card(approval, brightness: Brightness.light));
    expect(find.textContaining('Falta el detalle'), findsNothing);
    expect(find.text('Detalles técnicos'), findsNothing);
    expect(find.text('command Bash'), findsNothing);
    expect(
      find.widgetWithText(ComicButton, 'Permitir una vez'),
      findsOneWidget,
    );
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('the command and the tool name fold under technical details', (
    tester,
  ) async {
    await tester.pumpWidget(_card(_record(), brightness: Brightness.light));
    expect(find.text('Detalles técnicos'), findsOneWidget);
    expect(find.text('cargo test'), findsNothing);
    await tester.tap(find.text('Detalles técnicos'));
    await tester.pumpAndSettle();
    expect(find.text('cargo test'), findsOneWidget);
    expect(find.text('Ejecutar comando: cargo test'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('a high-risk shell card says it can change or delete files', (
    tester,
  ) async {
    final row = Map<String, Object?>.from(approvalRow())..['risk'] = 'high';
    final approval = parseApprovalRecord(json: recordJson(row))!;
    await tester.pumpWidget(_card(approval, brightness: Brightness.light));
    expect(find.text('Puede cambiar o borrar archivos.'), findsOneWidget);
    expect(find.text('riesgo alto'), findsNothing);
    await tester.pumpWidget(const SizedBox());
    await tester.pumpWidget(_card(_record(), brightness: Brightness.light));
    expect(find.text('Puede cambiar o borrar archivos.'), findsNothing);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets(
    'reduced motion holds the stamp on one tone, second after second',
    (tester) async {
      // The media query is read below MaterialApp, where the test can override it.
      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(HerdrPalettes.pergamino),
          builder: (context, child) => MediaQuery(
            data: MediaQuery.of(context).copyWith(disableAnimations: true),
            child: child!,
          ),
          home: Scaffold(
            body: ApprovalCard(
              approval: _record(),
              agentName: 'Bob',
              busy: false,
              failure: null,
              onDecide: (_) {},
            ),
          ),
        ),
      );
      final tag = find.byType(ComicTag);
      final hot = tester.element(tag).colors.hot;
      Color stamp() => tester
          .widget<ComicSlab>(
            find.descendant(of: tag, matching: find.byType(ComicSlab)),
          )
          .color;
      expect(stamp(), hot);
      for (var second = 0; second < 3; second++) {
        await tester.pump(const Duration(seconds: 1));
        expect(stamp(), hot);
      }
      await tester.pumpWidget(const SizedBox());
    },
  );

  group('a mail.move batch', () {
    ChatApproval moveRecord({
      String status = 'pending',
      List<Map<String, Object?>>? items,
    }) => parseApprovalRecord(
      json: recordJson(moveRow(status: status, items: items)),
    )!;

    Widget moveCard(
      ChatApproval approval, {
      Set<String> unchecked = const <String>{},
      void Function(String messageId, bool checked)? onToggle,
    }) => MaterialApp(
      theme: buildTheme(HerdrPalettes.pergamino),
      home: Scaffold(
        // The chat list scrolls, so a tall card is shown scrolled rather than clipped.
        body: SingleChildScrollView(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: ApprovalCard(
              approval: approval,
              agentName: 'Bob',
              busy: false,
              failure: null,
              onDecide: (_) {},
              unchecked: unchecked,
              onMoveToggle: onToggle ?? (_, _) {},
            ),
          ),
        ),
      ),
    );

    testWidgets('one checkbox per mail, counted in the button', (tester) async {
      await tester.pumpWidget(moveCard(moveRecord()));
      expect(find.text('Bob quiere mover correos'), findsOneWidget);
      expect(find.text('2 correos a mover'), findsOneWidget);
      expect(find.byType(CheckboxListTile), findsNWidgets(2));
      expect(
        find.widgetWithText(ComicButton, 'Mover los seleccionados (2 de 2)'),
        findsOneWidget,
      );
      expect(
        tester.getSize(find.byType(CheckboxListTile).first).height,
        greaterThanOrEqualTo(48),
      );
    });

    testWidgets('an unchecked mail lowers the count; none left disables it', (
      tester,
    ) async {
      await tester.pumpWidget(moveCard(moveRecord(), unchecked: {'m1'}));
      expect(
        find.widgetWithText(ComicButton, 'Mover los seleccionados (1 de 2)'),
        findsOneWidget,
      );
      await tester.pumpWidget(moveCard(moveRecord(), unchecked: {'m1', 'm2'}));
      expect(
        tester.widget<ComicButton>(find.byType(ComicButton).first).onPressed,
        isNull,
      );
      expect(find.text('Mover los seleccionados (0 de 2)'), findsOneWidget);
    });

    testWidgets('tapping a checkbox reports the mail and its new state', (
      tester,
    ) async {
      String? toggled;
      bool? checked;
      await tester.pumpWidget(
        moveCard(
          moveRecord(),
          onToggle: (messageId, value) {
            toggled = messageId;
            checked = value;
          },
        ),
      );
      await tester.tap(find.byType(CheckboxListTile).first);
      expect(toggled, 'm1');
      expect(checked, isFalse);
    });

    testWidgets('hostile mail text is shown as plain text', (tester) async {
      const subject = '<b>Gana</b>\nahora mismo';
      final reason = 'x' * 500;
      await tester.pumpWidget(
        moveCard(
          moveRecord(
            items: [
              moveItem('m1', subject: subject, reason: reason),
              moveItem('m2'),
            ],
          ),
        ),
      );
      expect(find.text(subject), findsOneWidget);
      expect(find.text(reason), findsOneWidget);
    });

    testWidgets(
      'a decided batch shows no checkboxes and marks the mails left out',
      (tester) async {
        await tester.pumpWidget(
          moveCard(
            moveRecord(
              status: 'approved',
              items: [moveItem('m1'), moveItem('m2', excluded: true)],
            ),
          ),
        );
        expect(find.byType(CheckboxListTile), findsNothing);
        expect(find.text('no se mueve'), findsOneWidget);
        expect(find.text('Permitida una vez'), findsOneWidget);
      },
    );

    testWidgets('a batch without its items asks to read the mail first', (
      tester,
    ) async {
      final bare = parseApprovalRecord(
        json: recordJson(approvalRow(tool: 'mail.move')),
      )!;
      await tester.pumpWidget(moveCard(bare));
      expect(find.byType(CheckboxListTile), findsNothing);
      expect(
        find.textContaining('No llegaron los datos del correo'),
        findsOneWidget,
      );
      expect(
        tester.widget<ComicButton>(find.byType(ComicButton).first).onPressed,
        isNull,
      );
    });
  });
}
