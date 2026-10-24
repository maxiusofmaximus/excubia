import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';
import 'package:herdr_notify/src/rust/api/state.dart';
import 'package:herdr_notify/ui/team_projects.dart';
import 'package:herdr_notify/ui/theme.dart';

AgentEntry agent({
  required String pane,
  required String project,
  required String title,
  AgentStatus status = AgentStatus.working,
  String cli = 'Claude',
}) => AgentEntry(
  paneId: pane,
  workspaceId: 'w1',
  project: project,
  agent: cli,
  status: status,
  attention: 0,
  title: title,
  focused: false,
  canResume: true,
  restored: false,
);

const project = ChatProject(
  id: 'prj_1',
  name: 'Demo',
  rootLabel: 'demo',
  status: 'active',
  lead: ChatSeat(
    agentId: 'ag_1',
    name: 'Ernesto',
    status: 'working',
    paneId: 'w1:p3',
  ),
);

void main() {
  test('agents are grouped by project and leftovers stay visible', () {
    final overview = groupTeam(
      projects: const [project],
      agents: [
        agent(pane: 'w1:p3', project: 'demo', title: 'Plan de tres hitos'),
        agent(pane: 'w1:p4', project: 'DEMO', title: 'Tests e2e'),
        agent(pane: 'w9:p1', project: 'Otro', title: 'Sin proyecto'),
      ],
    );

    final group = overview.projects.single;
    expect(group.project.name, 'Demo');
    expect(group.leadAgent?.paneId, 'w1:p3');
    expect(group.specialists.single.paneId, 'w1:p4');
    expect(group.seats, 2);
    expect(overview.unassigned.single.paneId, 'w9:p1');
  });

  test('a specialist follows the lead workspace when state reports no project',
      () {
    final overview = groupTeam(
      projects: const [project],
      agents: [
        agent(pane: 'w1:p3', project: 'apk6-test-host', title: 'Plan'),
        agent(pane: 'w1:p4', project: 'apk6-test-host', title: 'Parte uno'),
      ],
    );

    final group = overview.projects.single;
    expect(group.leadAgent?.paneId, 'w1:p3');
    expect(group.specialists.single.paneId, 'w1:p4');
    expect(group.seats, 2);
    expect(overview.unassigned, isEmpty);
  });

  testWidgets('a specialist name comes from the pane label before the CLI id', (
    tester,
  ) async {
    final overview = groupTeam(
      projects: const [project],
      agents: [
        agent(pane: 'w1:p3', project: 'Demo', title: 'Plan'),
        agent(
          pane: 'w1:p4',
          project: 'apk6-test-host',
          title: 'desconocido · Ana · especialista',
          cli: 'unknown',
        ),
      ],
    );
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Scaffold(
          body: TeamProjectGroupCard(
            group: overview.projects.single,
            onOpenAgent: (_) {},
          ),
        ),
      ),
    );

    expect(find.text('Ana'), findsOneWidget);
    expect(find.text('Unknown'), findsNothing);
  });

  testWidgets('a project shows its lead and specialists and opens the agent', (
    tester,
  ) async {
    String? opened;
    final overview = groupTeam(
      projects: const [project],
      agents: [
        agent(pane: 'w1:p3', project: 'Demo', title: 'Plan de tres hitos'),
        agent(pane: 'w1:p4', project: 'Demo', title: 'Tests e2e'),
      ],
    );
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Scaffold(
          body: SingleChildScrollView(
            child: TeamProjectGroupCard(
              group: overview.projects.single,
              onOpenAgent: (pane) => opened = pane,
            ),
          ),
        ),
      ),
    );

    expect(find.text('Demo'), findsOneWidget);
    expect(find.text('Ernesto'), findsOneWidget);
    expect(find.text('Lead'), findsOneWidget);
    expect(find.text('Claude'), findsOneWidget);
    expect(find.text('Especialista'), findsOneWidget);
    expect(find.textContaining('Plan de tres hitos'), findsOneWidget);
    expect(find.textContaining('Tests e2e'), findsOneWidget);

    await tester.tap(find.text('Ernesto'));
    expect(opened, 'w1:p3');
    await tester.tap(find.text('Claude'));
    expect(opened, 'w1:p4');
  });
}
