import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/src/rust/api/chat/model_team.dart';
import 'package:excubia/src/rust/api/state.dart';
import 'package:excubia/ui/team/team_controls.dart';
import 'package:excubia/ui/team_projects.dart';
import 'package:excubia/ui/theme.dart';

AgentEntry _agent(String pane, String title) => AgentEntry(
  paneId: pane,
  workspaceId: 'w1',
  project: 'Demo',
  agent: 'Claude',
  status: AgentStatus.working,
  attention: 0,
  title: title,
  focused: false,
  canResume: true,
  restored: false,
);

const _project = ChatProject(
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

final _agents = [
  _agent('w1:p3', 'Plan'),
  _agent(
    'w1:p4',
    'Tests e2e con un título bastante largo para forzar el ajuste',
  ),
];

Future<void> _pump(
  WidgetTester tester,
  Widget child, {
  double textScale = 1,
  bool disableAnimations = false,
}) async {
  tester.view.physicalSize = const Size(360, 640);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(context).copyWith(
          textScaler: TextScaler.linear(textScale),
          disableAnimations: disableAnimations,
        ),
        child: child!,
      ),
      home: Scaffold(body: SingleChildScrollView(child: child)),
    ),
  );
  await tester.pumpAndSettle();
}

Widget _card() {
  final group = groupTeam(
    projects: [_project],
    agents: _agents,
  ).projects.single;
  return TeamProjectGroupCard(group: group, onOpenAgent: (_) {});
}

/// The touch target of the nearest tappable ancestor of [text].
Size _touchOf(WidgetTester tester, String text) => tester.getSize(
  find
      .ancestor(of: find.text(text), matching: find.byType(GestureDetector))
      .first,
);

void main() {
  for (final scale in [1.0, 1.3]) {
    testWidgets(
      'the folder and its rows lay out at 360 dp, text scale $scale',
      (tester) async {
        await _pump(tester, _card(), textScale: scale);

        expect(tester.takeException(), isNull);
        expect(find.text('Demo'), findsOneWidget);
        expect(find.text('Ernesto'), findsOneWidget);
      },
    );

    testWidgets('the filter tabs lay out at 360 dp, text scale $scale', (
      tester,
    ) async {
      await _pump(
        tester,
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            TeamSearchPanel(
              controller: TextEditingController(),
              onChanged: (_) {},
            ),
            for (final label in ['Todos', 'Trabajando', 'Esperando'])
              TeamFilterTab(
                label: label,
                selected: label == 'Todos',
                onTap: () {},
              ),
          ],
        ),
        textScale: scale,
      );

      expect(tester.takeException(), isNull);
      expect(find.text('Trabajando'), findsOneWidget);
    });
  }

  testWidgets(
    'the folder, its rows and the filter tabs are at least 48 dp tall',
    (tester) async {
      await _pump(
        tester,
        Column(
          children: [
            _card(),
            Row(
              children: [
                TeamFilterTab(label: 'Todos', selected: true, onTap: () {}),
                TeamFilterTab(
                  label: 'Esperando',
                  selected: false,
                  onTap: () {},
                ),
              ],
            ),
          ],
        ),
      );

      expect(_touchOf(tester, 'Demo').height, greaterThanOrEqualTo(48));
      expect(_touchOf(tester, 'Ernesto').height, greaterThanOrEqualTo(48));
      expect(_touchOf(tester, 'Todos').height, greaterThanOrEqualTo(48));
      expect(_touchOf(tester, 'Esperando').height, greaterThanOrEqualTo(48));
      expect(_touchOf(tester, 'Esperando').width, greaterThanOrEqualTo(48));
    },
  );

  testWidgets('a tap on the folder folds and unfolds its agents', (
    tester,
  ) async {
    await _pump(tester, _card());
    expect(find.text('Ernesto'), findsOneWidget);

    await tester.tap(find.text('Demo'));
    await tester.pumpAndSettle();
    expect(find.text('Ernesto'), findsNothing);

    await tester.tap(find.text('Demo'));
    await tester.pumpAndSettle();
    expect(find.text('Ernesto'), findsOneWidget);
  });

  testWidgets('the tab has no circle or round clip', (tester) async {
    await _pump(tester, _card());

    expect(find.byType(CircleAvatar), findsNothing);
    expect(find.byType(ClipOval), findsNothing);
  });

  testWidgets('with reduced motion the chevron turns with no duration', (
    tester,
  ) async {
    await _pump(tester, _card(), disableAnimations: true);

    final turn = tester.widget<AnimatedRotation>(find.byType(AnimatedRotation));
    expect(turn.duration, Duration.zero);
  });
}
