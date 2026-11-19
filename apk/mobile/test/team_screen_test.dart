import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/chat_transport.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/plans_api.dart';
import 'package:excubia/core/api/recipes_api.dart';
import 'package:excubia/core/api/schedules_api.dart';
import 'package:excubia/core/api/team_summary_api.dart';
import 'package:excubia/core/state/chat_controller.dart';
import 'package:excubia/core/state/hires_controller.dart';
import 'package:excubia/core/state/live_triage.dart';
import 'package:excubia/core/state/plans_controller.dart';
import 'package:excubia/core/state/recipes_controller.dart';
import 'package:excubia/core/state/schedules_controller.dart';
import 'package:excubia/core/state/team_summary_controller.dart';
import 'package:excubia/src/rust/api/state.dart';
import 'package:excubia/src/rust/frb_generated.dart';
import 'package:excubia/ui/agents_screen.dart';
import 'package:excubia/ui/team/team_panel_parts.dart';
import 'package:excubia/ui/theme.dart';
import 'package:excubia/ui/widgets/agent_portrait_card.dart';
import 'package:excubia/ui/widgets/agent_row.dart';
import 'package:excubia/ui/widgets/grouped_card.dart';

import 'support/chat_fixtures.dart';
import 'support/connection_fixtures.dart';
import 'support/fake_chat_transport.dart';
import 'support/fake_team_host.dart';
import 'support/scripted_transport.dart';

const _emptyLine = 'Tu equipo está vacío';
const _noHires =
    'Todavía no has contratado a nadie. Pulsa «Contratar» para '
    'elegir por área.';
const _oneOfThree = [
  'Nadie espera tu visto bueno.',
  'Todavía no tienes a nadie contratado.',
  'Todavía no hay nadie contratado en tu equipo.',
  'Aún no hay planes propuestos.',
];

/// The team's own reads (summary and plans) with canned answers; the hires come
/// from the stateful team host, as in the team's other tests.
class TeamScreenHost implements ChatTransport {
  TeamScreenHost(
    this.team, {
    this.members = const [],
    this.plans = const [],
    this.failSummary = false,
    this.failPlans = false,
  });

  final FakeTeamHost team;
  final List<Map<String, Object?>> members;
  final List<Map<String, Object?>> plans;

  /// The summary or the plans answer with a server error.
  final bool failSummary;
  final bool failPlans;

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    if (target == '/v1/team/summary') {
      if (failSummary) throw const HostError(500, '{}');
      return jsonEncode({'ok': true, 'members': members});
    }
    if (target == '/v1/plans') {
      if (failPlans) throw const HostError(500, '{}');
      return jsonEncode({'ok': true, 'plans': plans});
    }
    return team.call(method, target, body);
  }

  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError();

  @override
  Future<List<int>> callBytes(String method, String target) =>
      throw UnimplementedError();
}

/// The Equipo tab's controllers, so a test can answer the reads after the screen is up.
class TeamParts {
  TeamParts(this.hires, this.summary, this.plans);

  final HiresController hires;
  final TeamSummaryController summary;
  final PlansController plans;
}

/// The Equipo tab with the given team. The PC is out of reach, as in the
/// connection tests, so the list of live agents stays empty.
Future<TeamParts> pumpEquipo(
  WidgetTester tester,
  TeamScreenHost host, {
  bool readHires = true,
  bool readSummaryBeforeShow = true,
  VoidCallback? onOpenChat,
  List<Map<String, Object?>> projects = const [],
  List<AgentEntry> agents = const [],
}) async {
  final client = refusingClient();
  final triage = LiveTriage(client);
  // The projects come from the chat bootstrap, read like the screen does at start.
  final chat = ChatController(
    FakeChatTransport({
      ...emptyRoutes(),
      '/v1/chat/bootstrap': bootstrapJson(projects: projects),
    }),
    triage,
  );
  final hires = HiresController(transport: host.team);
  final summary = TeamSummaryController(api: TeamSummaryApi(host));
  final plans = PlansController(api: PlansApi(host));
  final recipes = RecipesController(
    api: RecipesApi(host.team),
    conversationId: () => null,
  );
  await tester.runAsync(() async {
    triage.start();
    await settled(triage);
    triage.stop();
    if (agents.isNotEmpty) {
      triage.reconciler.acceptState(
        StateSnapshot(revision: 1, counts: const {}, agents: agents),
      );
    }
    await chat.start();
  });
  if (readHires) await hires.load();
  if (readSummaryBeforeShow) {
    await summary.read();
    await plans.read();
  }
  addTearDown(triage.dispose);
  addTearDown(chat.dispose);

  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      home: MediaQuery(
        data: const MediaQueryData(disableAnimations: true),
        child: AgentsScreen(
          triage: triage,
          chat: chat,
          hires: hires,
          recipes: recipes,
          summary: summary,
          plans: plans,
          schedules: SchedulesController(
            api: SchedulesApi(ScriptedTransport(const {})),
          ),
          onUnpaired: () async {},
          onOpenChat: onOpenChat,
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return TeamParts(hires, summary, plans);
}

/// The one empty state and none of the team's sections; [notices] small notices above it.
void expectOnlyEmptyState({int notices = 0}) {
  expect(find.text(_emptyLine), findsOneWidget);
  expect(find.byType(TeamNoticeBox), findsNWidgets(notices));
  for (final text in [
    ..._oneOfThree,
    _noHires,
    'TU EQUIPO',
    'CONTRATADOS',
    'PLANES PROPUESTOS',
    'Todos',
    'Trabajando',
    'Esperando',
  ]) {
    expect(find.text(text), findsNothing, reason: text);
  }
  expect(find.byType(TextField), findsNothing, reason: 'no search while empty');
}

/// Open agents on the PC, none of them in a project card.
List<AgentEntry> openAgents(int count) => [
  for (var i = 1; i <= count; i++)
    AgentEntry(
      paneId: 'w9:p$i',
      workspaceId: 'w9',
      project: 'excubia',
      agent: 'opencode',
      status: AgentStatus.idle,
      attention: 0,
      title: 'Tarea $i',
      cwd: null,
      focused: false,
      canResume: false,
      restored: false,
    ),
];

/// Project cards from the chat bootstrap, each with its own lead.
List<Map<String, Object?>> projectCards(int count) => [
  for (var i = 1; i <= count; i++)
    {
      'id': 'prj_$i',
      'name': 'Proyecto $i',
      'root_label': 'proyecto-$i',
      'status': 'active',
      'lead': {
        'agent_id': 'ag_$i',
        'name': 'Lead $i',
        'status': 'working',
        'pane_id': 'wz$i:p1',
      },
    },
];

void main() {
  setUpAll(RustLib.init);

  testWidgets('four entries (agents and cards): no search and no chips', (
    tester,
  ) async {
    // Tall enough that both agent rows are built: the list is lazy.
    tester.view.physicalSize = const Size(400, 1400);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await pumpEquipo(
      tester,
      TeamScreenHost(FakeTeamHost(hires: [])),
      projects: projectCards(2),
      agents: openAgents(2),
    );

    expect(find.byType(AgentRow), findsNWidgets(2));
    expect(find.byType(TextField), findsNothing);
    expect(find.text('Todos'), findsNothing);
  });

  testWidgets(
    'five entries (agents and cards): the search and the chips show',
    (tester) async {
      await pumpEquipo(
        tester,
        TeamScreenHost(FakeTeamHost(hires: [])),
        projects: projectCards(2),
        agents: openAgents(3),
      );

      expect(find.byType(TextField), findsOneWidget);
      expect(find.text('Todos'), findsOneWidget);
    },
  );

  testWidgets('a phone that cannot contract shows no CONTRATADOS section', (
    tester,
  ) async {
    await pumpEquipo(
      tester,
      TeamScreenHost(FakeTeamHost(canHire: false, hires: [])),
      projects: projectCards(1),
    );

    expect(find.text('Proyecto 1'), findsOneWidget);
    expect(find.text('CONTRATADOS'), findsNothing);
    expect(find.text(_noHires), findsNothing);
    expect(
      find.text('Todavía no hay nadie contratado en tu equipo.'),
      findsNothing,
    );
  });

  testWidgets('an empty team says so once, with the way to contract', (
    tester,
  ) async {
    await pumpEquipo(tester, TeamScreenHost(FakeTeamHost(hires: [])));

    expect(find.text(_emptyLine), findsOneWidget);
    expect(find.text('Pídele a Alfred que arme uno'), findsOneWidget);
    expect(find.text('Contratar'), findsOneWidget);
    for (final text in [..._oneOfThree, _noHires]) {
      expect(find.text(text), findsNothing, reason: text);
    }
  });

  testWidgets(
    'projects and nothing else: the cards, never «vacío» above them',
    (tester) async {
      await pumpEquipo(
        tester,
        TeamScreenHost(FakeTeamHost(hires: [])),
        projects: [
          {
            'id': 'prj_1',
            'name': 'Demo',
            'root_label': 'demo',
            'status': 'active',
            'lead': {
              'agent_id': 'ag_1',
              'name': 'Ernesto',
              'status': 'working',
              'pane_id': 'w1:p3',
            },
          },
        ],
      );

      expect(find.text(_emptyLine), findsNothing);
      expect(find.text('Pídele a Alfred que arme uno'), findsNothing);
      expect(find.text('Hablar con Alfred'), findsNothing);
      expect(find.text('Demo'), findsOneWidget);
      expect(find.text('Ernesto'), findsOneWidget);
      for (final text in [
        'TU EQUIPO',
        'PLANES PROPUESTOS',
        'Todavía no tienes a nadie contratado.',
        'Nadie trabajando en este proyecto.',
      ]) {
        expect(find.text(text), findsNothing, reason: text);
      }
    },
  );

  testWidgets(
    'a phone that cannot contract offers the chat, not a dead button',
    (tester) async {
      var opened = 0;
      await pumpEquipo(
        tester,
        TeamScreenHost(FakeTeamHost(canHire: false, hires: [])),
        onOpenChat: () => opened++,
      );

      expect(find.text('Pídele a Alfred que arme uno'), findsOneWidget);
      expect(find.text('Contratar'), findsNothing);
      await tester.tap(find.text('Hablar con Alfred'));
      expect(opened, 1);
    },
  );

  testWidgets(
    'without a way to the chat, a phone that cannot contract shows no button',
    (tester) async {
      await pumpEquipo(
        tester,
        TeamScreenHost(FakeTeamHost(canHire: false, hires: [])),
      );

      expect(find.text('Pídele a Alfred que arme uno'), findsOneWidget);
      expect(find.text('Contratar'), findsNothing);
      expect(find.text('Hablar con Alfred'), findsNothing);
    },
  );

  testWidgets(
    'until the team is read, no empty state: the loading block only',
    (tester) async {
      final semantics = tester.ensureSemantics();
      final hires = (await pumpEquipo(
        tester,
        TeamScreenHost(FakeTeamHost(hires: [])),
        readHires: false,
      )).hires;

      expect(find.text(_emptyLine), findsNothing);
      for (final text in [..._oneOfThree, _noHires]) {
        expect(find.text(text), findsNothing, reason: text);
      }
      expect(find.bySemanticsLabel('Cargando tu equipo'), findsOneWidget);

      await hires.load();
      await tester.pumpAndSettle();

      expect(find.text(_emptyLine), findsOneWidget);
      expect(find.bySemanticsLabel('Cargando tu equipo'), findsNothing);
      semantics.dispose();
    },
  );

  testWidgets(
    'a contracted person is never under the empty state, even while reading',
    (tester) async {
      final hires = (await pumpEquipo(
        tester,
        TeamScreenHost(FakeTeamHost(hires: [bobHire()])),
        readHires: false,
      )).hires;
      expect(find.text(_emptyLine), findsNothing);

      await hires.load();
      await tester.pumpAndSettle();

      expect(find.text(_emptyLine), findsNothing);
      expect(find.text('Bob'), findsWidgets);
    },
  );

  testWidgets('a contracted person gets a portrait, and it opens her sheet', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(400, 1000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await pumpEquipo(tester, TeamScreenHost(FakeTeamHost(hires: [bobHire()])));

    expect(find.text(_emptyLine), findsNothing);
    expect(
      find.descendant(of: find.byType(GroupedCard), matching: find.text('Bob')),
      findsOneWidget,
      reason: 'the list under «Contratados» holds her too',
    );
    // The row sits below the plans and the summary of a lazy list.
    await tester.scrollUntilVisible(
      find.byType(AgentPortraitCard),
      200,
      scrollable: find.byType(Scrollable).first,
    );
    expect(find.byType(AgentPortraitCard), findsOneWidget);

    await tester.tap(find.byType(AgentPortraitCard));
    await tester.pumpAndSettle();

    expect(find.text('Renombrar'), findsOneWidget);
    expect(
      find.byType(AgentPortraitCard),
      findsNWidgets(2),
      reason: 'the sheet of the person shows her portrait as well',
    );
  });

  testWidgets('with nobody contracted there is no portrait row', (
    tester,
  ) async {
    await pumpEquipo(tester, TeamScreenHost(FakeTeamHost(hires: [])));

    expect(find.byType(AgentPortraitCard), findsNothing);
  });

  testWidgets(
    'the empty state shows once the summary and the plans answer, after the screen is up',
    (tester) async {
      // The screen is built while the summary and the plans are still out: only their
      // own notifications bring the answers, so the screen must listen to them too.
      final team = await pumpEquipo(
        tester,
        TeamScreenHost(FakeTeamHost(hires: [])),
        readSummaryBeforeShow: false,
      );
      expect(find.text(_emptyLine), findsNothing);

      await team.summary.refresh();
      await team.plans.refresh();
      await tester.pumpAndSettle();

      expectOnlyEmptyState();
    },
  );

  testWidgets('a failed summary read: one notice above the empty state', (
    tester,
  ) async {
    await pumpEquipo(
      tester,
      TeamScreenHost(FakeTeamHost(hires: []), failSummary: true),
    );

    expectOnlyEmptyState(notices: 1);
  });

  testWidgets('a failed plans read: one notice above the empty state', (
    tester,
  ) async {
    await pumpEquipo(
      tester,
      TeamScreenHost(FakeTeamHost(hires: []), failPlans: true),
    );

    expectOnlyEmptyState(notices: 1);
  });

  testWidgets('a failed read of the team: one notice above the empty state', (
    tester,
  ) async {
    await pumpEquipo(
      tester,
      TeamScreenHost(FakeTeamHost(hires: [])..networkDown = true),
    );

    expectOnlyEmptyState(notices: 1);
  });

  testWidgets('two failed reads still give one notice and one empty state', (
    tester,
  ) async {
    await pumpEquipo(
      tester,
      TeamScreenHost(
        FakeTeamHost(hires: [])..networkDown = true,
        failSummary: true,
        failPlans: true,
      ),
    );

    expectOnlyEmptyState(notices: 1);
  });
}
