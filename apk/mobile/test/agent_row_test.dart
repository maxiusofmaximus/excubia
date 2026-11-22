import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/src/rust/api/state.dart';
import 'package:excubia/ui/theme.dart';
import 'package:excubia/ui/widgets/agent_row.dart';

import 'support/agent_fixtures.dart';

Future<void> pumpRow(WidgetTester tester, AgentEntry entry) =>
    tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Scaffold(
          body: AgentRow(agent: entry, onTap: () {}),
        ),
      ),
    );

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

    expect(find.text('bench'), findsOneWidget);
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

  testWidgets('a finished agent with no task line says when it ended', (
    tester,
  ) async {
    final twoHoursAgo = DateTime.now()
        .subtract(const Duration(hours: 2))
        .millisecondsSinceEpoch;
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Scaffold(
          body: AgentRow(
            agent: agent(
              status: AgentStatus.unknown,
              restored: true,
              lastActiveMs: twoHoursAgo,
            ),
            onTap: () {},
          ),
        ),
      ),
    );

    expect(find.text('Terminó hace 2 h'), findsOneWidget);
    expect(find.text('Sin actividad reciente'), findsNothing);
  });

  testWidgets('a finished agent without a timestamp shows no line', (
    tester,
  ) async {
    await pumpRow(tester, agent(status: AgentStatus.unknown, restored: true));

    expect(find.textContaining('Terminó'), findsNothing);
    expect(find.text('bench'), findsOneWidget);
  });

  testWidgets('a finished agent with a task keeps its task line', (
    tester,
  ) async {
    final twoHoursAgo = DateTime.now()
        .subtract(const Duration(hours: 2))
        .millisecondsSinceEpoch;
    await pumpRow(
      tester,
      agent(
        status: AgentStatus.idle,
        title: 'Plan de tres hitos',
        restored: true,
        lastActiveMs: twoHoursAgo,
      ),
    );

    expect(find.text('inactivo · Plan de tres hitos'), findsOneWidget);
    expect(find.textContaining('Terminó'), findsNothing);
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

      expect(find.text('bench'), findsOneWidget);
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

  testWidgets(
    'an agent with no project of its own shows its character, not the app name',
    (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(),
          home: Scaffold(
            body: AgentRow(
              agent: agent(
                status: AgentStatus.idle,
                title: 'Ernesto — lead',
                project: 'Excubia',
                cli: 'unknown',
              ),
              onTap: () {},
            ),
          ),
        ),
      );

      expect(find.text('Ernesto'), findsOneWidget);
      expect(find.text('Excubia'), findsNothing);
    },
  );
}
