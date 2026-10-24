import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/src/rust/api/state.dart';
import 'package:herdr_notify/ui/theme.dart';
import 'package:herdr_notify/ui/widgets/agent_row.dart';

AgentEntry agent({required AgentStatus status, String? title}) => AgentEntry(
  paneId: 'w1:p1',
  workspaceId: 'w1',
  project: 'herdr-notify',
  agent: 'opencode',
  status: status,
  attention: 3,
  title: title,
  cwd: '/repo',
  focused: false,
  canResume: false,
  restored: false,
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

    expect(find.text('herdr-notify'), findsOneWidget);
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
}
