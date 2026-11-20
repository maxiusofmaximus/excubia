import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/src/rust/api/state.dart';
import 'package:excubia/ui/theme.dart';
import 'package:excubia/ui/widgets/agent_row.dart';

import 'support/agent_fixtures.dart';

void main() {
  testWidgets('a row shows project and task over a 56dp-plus touch target', (
    tester,
  ) async {
    var taps = 0;
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Scaffold(
          body: AgentRow(
            agent: agent(
              status: AgentStatus.blocked,
              title: 'Add the pairing screen',
            ),
            onTap: () => taps++,
          ),
        ),
      ),
    );

    expect(find.text('excubia'), findsOneWidget);
    expect(find.text('bloqueado · Add the pairing screen'), findsOneWidget);
    final row = tester.getSize(find.byType(AgentRow));
    expect(row.height, greaterThanOrEqualTo(64));

    await tester.tap(find.byType(AgentRow));
    expect(taps, 1);
  });

  testWidgets('a row without a task falls back to the status word', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Scaffold(
          body: AgentRow(
            agent: agent(status: AgentStatus.idle),
            onTap: () {},
          ),
        ),
      ),
    );

    expect(find.text('inactivo'), findsOneWidget);
  });

  testWidgets(
    'a loose agent with no activity never reads «Sin actividad reciente»',
    (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(),
          home: Scaffold(
            body: AgentRow(
              agent: agent(status: AgentStatus.unknown),
              onTap: () {},
            ),
          ),
        ),
      );

      expect(find.text('excubia'), findsOneWidget);
      expect(find.text('Sin actividad reciente'), findsNothing);
    },
  );

  testWidgets(
    'a role tag in the title is not shown: «Plan de tres hitos — lead» reads «Plan de tres hitos»',
    (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(),
          home: Scaffold(
            body: AgentRow(
              agent: agent(
                status: AgentStatus.unknown,
                title: 'Plan de tres hitos — lead',
              ),
              onTap: () {},
            ),
          ),
        ),
      );

      expect(find.text('Plan de tres hitos'), findsOneWidget);
      expect(find.textContaining('lead'), findsNothing);
      expect(find.text('Sin actividad reciente'), findsNothing);
    },
  );
}
