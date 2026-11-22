import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/alerts_api.dart';
import 'package:excubia/ui/chat/chat_screen.dart';
import 'package:excubia/ui/theme.dart';

import 'support/scripted_transport.dart';

/// The header at a phone's 360 dp width, at the given text scale.
Widget _header({required int pending, required double textScale}) =>
    MaterialApp(
      theme: buildTheme(),
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(context)
            .copyWith(textScaler: TextScaler.linear(textScale)),
        child: child!,
      ),
      home: Scaffold(
        body: SizedBox(
          width: 360,
          child: ChatHeader(
            name: 'Alfred',
            status: 'Disponible',
            offline: false,
            unread: 0,
            approvalsPending: pending,
            onApprovals: () {},
            onHistory: () {},
            onNewChat: () {},
            alertRules: AlertsApi(ScriptedTransport({})),
          ),
        ),
      ),
    );

void main() {
  for (final scale in [1.0, 1.3]) {
    testWidgets(
      'with a pending approval the name stays on one line at text scale $scale',
      (tester) async {
        await tester.pumpWidget(_header(pending: 1, textScale: scale));

        final name = tester.getSize(find.text('ALFRED'));
        // One line: a broken name is about twice its font height.
        expect(name.height, lessThan(1.5 * 12 * scale));
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets('with a pending approval the status reads whole at 360 dp', (
    tester,
  ) async {
    await tester.pumpWidget(_header(pending: 1, textScale: 1.0));

    final status = tester.renderObject<RenderParagraph>(
      find.text('Disponible'),
    );
    expect(status.didExceedMaxLines, isFalse);
  });

  testWidgets('the shield says how many approvals wait, and keeps 48 dp', (
    tester,
  ) async {
    await tester.pumpWidget(_header(pending: 1, textScale: 1.0));
    expect(find.bySemanticsLabel('1 permiso pendiente'), findsOneWidget);
    expect(find.byTooltip('1 permiso pendiente'), findsOneWidget);
    expect(tester.getSize(find.byTooltip('1 permiso pendiente')).width, 48);

    await tester.pumpWidget(_header(pending: 3, textScale: 1.0));
    expect(find.byTooltip('3 permisos pendientes'), findsOneWidget);
  });

  testWidgets('without a pending approval the header has no shield', (
    tester,
  ) async {
    await tester.pumpWidget(_header(pending: 0, textScale: 1.0));
    expect(find.byIcon(Icons.shield_outlined), findsNothing);
    expect(find.byTooltip('Nuevo chat'), findsOneWidget);
  });
}
