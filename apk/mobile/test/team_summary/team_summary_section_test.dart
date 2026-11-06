import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/chat_transport.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/api/plans_api.dart';
import 'package:herdr_notify/core/api/team_summary_api.dart';
import 'package:herdr_notify/core/api/recipes_api.dart';
import 'package:herdr_notify/core/state/hires_controller.dart';
import 'package:herdr_notify/core/state/recipes_controller.dart';
import 'package:herdr_notify/core/state/plans_controller.dart';
import 'package:herdr_notify/core/state/team_summary_controller.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/team/team_summary_section.dart';
import 'package:herdr_notify/ui/theme.dart';

import '../support/fake_team_host.dart';

/// Answers `GET /v1/team/summary` with [members], or throws [error] when it is
/// set. Every other route goes to the team host, so the hires are real.
class SummaryHost implements ChatTransport {
  SummaryHost(this.team, {this.members = const [], this.error});

  final FakeTeamHost team;
  List<Map<String, Object?>> members;
  HostError? error;

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    if (target == '/v1/team/summary') {
      if (error != null) throw error!;
      return jsonEncode({'ok': true, 'members': members});
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

Map<String, Object?> summaryMember({
  String id = 'hire_1',
  String name = 'Bob',
  String status = 'idle',
  String headline = 'Sin actividad',
  String? since = '2026-11-05T10:00:00.000Z',
}) => {
  'agent_id': id,
  'name': name,
  'role_title': 'Programador',
  'status': status,
  'headline': headline,
  'since': ?since,
};

class SectionScreen {
  SectionScreen(this.summary, this.router);

  final TeamSummaryController summary;
  final SummaryHost router;
}

/// Mounts the section after one read of the summary, at [width] dp.
Future<SectionScreen> pumpSection(
  WidgetTester tester, {
  FakeTeamHost? team,
  List<Map<String, Object?>> members = const [],
  HostError? error,
  double width = 800,
  double textScale = 1,
}) async {
  final hostTeam = team ?? FakeTeamHost();
  final router = SummaryHost(hostTeam, members: members, error: error);
  final hires = HiresController(transport: hostTeam);
  await hires.load();
  final summary = TeamSummaryController(
    api: TeamSummaryApi(router),
    interval: const Duration(minutes: 1),
  );
  await summary.refresh();
  addTearDown(summary.dispose);
  final plans = PlansController(
    api: PlansApi(router),
    interval: const Duration(minutes: 1),
  );
  addTearDown(plans.dispose);
  addTearDown(hires.dispose);
  tester.view.physicalSize = Size(width * 3, 2400 * 3);
  tester.view.devicePixelRatio = 3;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(context)
            .copyWith(textScaler: TextScaler.linear(textScale)),
        child: child!,
      ),
      home: Scaffold(
        body: ListView(
          children: [
            TeamSummarySection(
              summary: summary,
              hires: hires,
              recipes: RecipesController(
                api: RecipesApi(FakeTeamHost()),
                conversationId: () => null,
              ),
              plans: plans,
            ),
          ],
        ),
      ),
    ),
  );
  await tester.pump();
  return SectionScreen(summary, router);
}

void main() {
  setUpAll(RustLib.init);

  group('the list', () {
    testWidgets('one card per member, in the order the host sent', (
      tester,
    ) async {
      await pumpSection(
        tester,
        team: FakeTeamHost(
          hires: [
            bobHire(id: 'hire_1', name: 'Bob'),
            bobHire(id: 'hire_2', name: 'Vera'),
          ],
        ),
        members: [
          summaryMember(
            id: 'hire_2',
            name: 'Vera',
            status: 'waiting_for_you',
            headline: 'Quiere revisar el plan',
          ),
          summaryMember(id: 'hire_1', name: 'Bob', status: 'working'),
        ],
      );

      expect(find.text('Quiere revisar el plan'), findsOneWidget);
      expect(find.text('Espera tu visto bueno'), findsOneWidget);
      expect(find.text('Trabajando'), findsOneWidget);
      expect(
        tester.getTopLeft(find.text('Vera')).dy,
        lessThan(tester.getTopLeft(find.text('Bob')).dy),
      );
    });

    testWidgets('the counter says how many wait, in a live region', (
      tester,
    ) async {
      await pumpSection(
        tester,
        team: FakeTeamHost(hires: [bobHire()]),
        members: [
          summaryMember(status: 'waiting_for_you'),
          summaryMember(id: 'hire_2', name: 'Vera', status: 'waiting_for_you'),
        ],
      );

      expect(find.text('2 esperan tu visto bueno'), findsOneWidget);
      expect(
        find.ancestor(
          of: find.text('2 esperan tu visto bueno'),
          matching: find.byWidgetPredicate(
            (widget) =>
                widget is Semantics && widget.properties.liveRegion == true,
          ),
        ),
        findsWidgets,
      );
    });

    testWidgets(
      'the age reads «desde hace» and a long headline does not break',
      (tester) async {
        final since = DateTime.now().toUtc().subtract(
          const Duration(minutes: 12),
        );
        await pumpSection(
          tester,
          team: FakeTeamHost(hires: [bobHire()]),
          members: [
            summaryMember(
              status: 'blocked',
              headline: 'Necesita que confirmes la fecha de entrega del informe anual',
              since: since.toIso8601String(),
            ),
          ],
        );

        expect(find.text('Necesita ayuda'), findsOneWidget);
        expect(find.textContaining('desde hace 12 min'), findsOneWidget);
        expect(tester.takeException(), isNull);
      },
    );
  });

  group('the empty and failed states', () {
    testWidgets('an empty team with can_hire invites to contract', (
      tester,
    ) async {
      await pumpSection(tester);

      expect(
        find.text('Todavía no tienes a nadie contratado.'),
        findsOneWidget,
      );
      expect(
        find.text(
          'Contrata a quien necesites y verás aquí en qué anda cada uno.',
        ),
        findsOneWidget,
      );
      expect(find.widgetWithText(TextButton, 'Contratar'), findsOneWidget);
    });

    testWidgets('an empty team without can_hire offers nothing', (
      tester,
    ) async {
      await pumpSection(tester, team: FakeTeamHost(canHire: false));

      expect(find.textContaining('Todavía no tienes a nadie'), findsOneWidget);
      expect(find.text('Contratar'), findsNothing);
    });

    testWidgets('a failed read with no cards offers a retry that reads again', (
      tester,
    ) async {
      final screen = await pumpSection(
        tester,
        team: FakeTeamHost(hires: [bobHire()]),
        error: const HostError(500, '{}'),
      );

      expect(
        find.text('No se pudo leer tu equipo. Inténtalo de nuevo.'),
        findsOneWidget,
      );
      expect(find.text('Nadie espera tu visto bueno.'), findsNothing);
      screen.router
        ..error = null
        ..members = [summaryMember()];
      await tester.tap(find.text('Reintentar'));
      await tester.pumpAndSettle();

      expect(find.text('Bob'), findsOneWidget);
      expect(find.text('Reintentar'), findsNothing);
    });

    testWidgets('a 403 says the device waits for its grant, with no retry', (
      tester,
    ) async {
      await pumpSection(
        tester,
        team: FakeTeamHost(hires: [bobHire()]),
        error: const HostError(403, '{}'),
      );

      expect(
        find.text('Esperando que aprueben este dispositivo.'),
        findsOneWidget,
      );
      expect(find.text('Reintentar'), findsNothing);
    });

    testWidgets('a failed read after good ones keeps the cards on screen', (
      tester,
    ) async {
      final screen = await pumpSection(
        tester,
        team: FakeTeamHost(hires: [bobHire()]),
        members: [summaryMember()],
      );
      expect(find.text('Nadie espera tu visto bueno.'), findsOneWidget);
      screen.router.error = const HostError.transport(null, '');
      await screen.summary.refresh();
      await tester.pump();

      expect(find.text('Bob'), findsOneWidget);
      expect(find.text('Reintentar'), findsOneWidget);
      expect(find.text('Nadie espera tu visto bueno.'), findsNothing);
    });
  });

  group('the card', () {
    testWidgets('tapping a hire with can_hire opens the person', (
      tester,
    ) async {
      await pumpSection(
        tester,
        team: FakeTeamHost(hires: [bobHire()]),
        members: [summaryMember()],
      );

      await tester.tap(find.text('Bob'));
      await tester.pumpAndSettle();

      expect(find.text('Renombrar'), findsOneWidget);
    });

    testWidgets('without can_hire a card cannot be opened', (tester) async {
      await pumpSection(
        tester,
        team: FakeTeamHost(canHire: false, hires: [bobHire()]),
        members: [summaryMember()],
      );

      await tester.tap(find.text('Bob'));
      await tester.pumpAndSettle();

      expect(find.text('Renombrar'), findsNothing);
    });

    testWidgets('an idle card says «Sin actividad» once, not as a headline too', (
      tester,
    ) async {
      await pumpSection(
        tester,
        team: FakeTeamHost(hires: [bobHire()]),
        members: [summaryMember(headline: 'sin actividad.')],
      );

      expect(
        find.byWidgetPredicate(
          (widget) =>
              widget is Text &&
              (widget.data ?? '').toLowerCase().contains('actividad'),
        ),
        findsOneWidget,
      );
    });
  });

  testWidgets('at 360 dp with the text at 1.3× nothing overflows', (
    tester,
  ) async {
    await pumpSection(
      tester,
      team: FakeTeamHost(hires: [bobHire()]),
      members: [
        summaryMember(
          status: 'waiting_for_you',
          headline: 'Necesita que confirmes la fecha de entrega del informe anual antes del viernes',
        ),
      ],
      width: 360,
      textScale: 1.3,
    );

    expect(tester.takeException(), isNull);
  });
}
