import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/plans_api.dart';
import 'package:excubia/core/api/recipes_api.dart';
import 'package:excubia/core/api/team_summary_api.dart';
import 'package:excubia/core/state/hires_controller.dart';
import 'package:excubia/core/state/plans_controller.dart';
import 'package:excubia/core/state/recipes_controller.dart';
import 'package:excubia/core/state/team_summary_controller.dart';
import 'package:excubia/src/rust/frb_generated.dart';
import 'package:excubia/ui/plans/plans_section.dart';
import 'package:excubia/ui/team/team_summary_section.dart';
import 'package:excubia/ui/theme.dart';

import '../support/fake_team_host.dart';
import '../support/scripted_transport.dart';

Map<String, Object?> _plan(
  String id,
  String status, {
  List<String> touches = const ['files'],
  String? updatedAt,
  String? note,
}) => {
  'id': id,
  'status': status,
  'steps': [
    {'title': 'Leer el repo', 'touches': touches},
    {'title': 'Resumir', 'touches': const <String>[]},
  ],
  'summary': 'Plan de 2 pasos · lo propone Ernesto',
  'created_at': '2026-11-05T10:00:00.000Z',
  'updated_at': ?updatedAt,
  'note': ?note,
};

String _list(List<Map<String, Object?>> plans) =>
    jsonEncode({'ok': true, 'plans': plans});

String _answer(Map<String, Object?> plan) =>
    jsonEncode({'ok': true, 'plan': plan});

/// Mounts the section after one read of the plans, at [width] dp.
Future<PlansController> pumpPlans(
  WidgetTester tester,
  ScriptedTransport host, {
  double width = 390,
  double textScale = 1,
}) async {
  final plans = PlansController(
    api: PlansApi(host),
    interval: const Duration(hours: 1),
  );
  addTearDown(plans.dispose);
  await plans.refresh();
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
        body: ListView(children: [PlansSection(plans: plans)]),
      ),
    ),
  );
  await tester.pump();
  return plans;
}

void main() {
  setUpAll(RustLib.init);

  group('the states of the section', () {
    testWidgets('no plans says when one will show up', (tester) async {
      await pumpPlans(tester, ScriptedTransport({'GET /v1/plans': _list([])}));

      expect(
        find.text('Cuando un agente proponga un plan, lo verás aquí.'),
        findsOneWidget,
      );
    });

    testWidgets('a failed first read offers Reintentar, which reads again', (
      tester,
    ) async {
      final host = ScriptedTransport({
        'GET /v1/plans': _list([_plan('plan_a', 'proposed')]),
      });
      host.failNext('GET /v1/plans', const HostError(500, '{}'));
      await pumpPlans(tester, host);

      expect(
        find.text('No se pudo leer los planes. Inténtalo de nuevo.'),
        findsOneWidget,
      );
      await tester.tap(find.widgetWithText(TextButton, 'Reintentar'));
      await tester.pumpAndSettle();

      expect(find.text('Reintentar'), findsNothing);
      expect(find.text('Leer el repo'), findsOneWidget);
    });

    testWidgets(
      'a read the host refuses says the device is waiting for its grant',
      (tester) async {
        final host = ScriptedTransport({});
        host.failNext('GET /v1/plans', const HostError(403, '{}'));
        await pumpPlans(tester, host);

        expect(
          find.text('Esperando que aprueben este dispositivo.'),
          findsOneWidget,
        );
        expect(find.text('Reintentar'), findsNothing);
      },
    );
  });

  group('a proposal', () {
    testWidgets('shows who proposes it, its steps and its three answers', (
      tester,
    ) async {
      await pumpPlans(
        tester,
        ScriptedTransport({
          'GET /v1/plans': _list([_plan('plan_a', 'proposed')]),
        }),
      );

      expect(find.text('Plan de 2 pasos · lo propone Ernesto'), findsOneWidget);
      expect(find.text('Leer el repo'), findsOneWidget);
      expect(find.text('Resumir'), findsOneWidget);
      expect(find.text('Toca ficheros'), findsOneWidget);
      expect(find.widgetWithText(FilledButton, 'Aprobar'), findsOneWidget);
      expect(find.widgetWithText(OutlinedButton, 'Cambiar…'), findsOneWidget);
      expect(find.widgetWithText(TextButton, 'Rechazar'), findsOneWidget);
    });

    testWidgets(
      'a decided plan comes after the open ones, with its status and no answers',
      (tester) async {
        await pumpPlans(
          tester,
          ScriptedTransport({
            'GET /v1/plans': _list([
              _plan('done', 'approved', updatedAt: '2026-11-04T09:00:00.000Z'),
              _plan('open', 'proposed'),
            ]),
          }),
        );

        expect(find.text('Aprobado'), findsOneWidget);
        expect(find.widgetWithText(FilledButton, 'Aprobar'), findsOneWidget);
        expect(
          tester.getTopLeft(find.text('Aprobado')).dy,
          greaterThan(
            tester.getTopLeft(find.widgetWithText(FilledButton, 'Aprobar')).dy,
          ),
        );
      },
    );

    testWidgets('approving a plan with no mail or internet sends it at once', (
      tester,
    ) async {
      final host = ScriptedTransport({
        'GET /v1/plans': _list([_plan('plan_a', 'proposed')]),
        'POST /v1/plans/plan_a/approve': _answer(_plan('plan_a', 'approved')),
      });
      await pumpPlans(tester, host);

      await tester.tap(find.widgetWithText(FilledButton, 'Aprobar'));
      await tester.pumpAndSettle();

      expect(host.calls, contains('POST /v1/plans/plan_a/approve'));
      expect(find.text('Aprobado'), findsOneWidget);
    });

    testWidgets(
      'a plan that touches mail asks first, and «Volver» keeps it open',
      (tester) async {
        final host = ScriptedTransport({
          'GET /v1/plans': _list([
            _plan('plan_a', 'proposed', touches: ['mail']),
          ]),
        });
        await pumpPlans(tester, host);

        await tester.tap(find.widgetWithText(FilledButton, 'Aprobar'));
        await tester.pumpAndSettle();
        expect(
          find.text(
            'Este plan toca correo o internet. Revisa los pasos antes de aprobarlo.',
          ),
          findsOneWidget,
        );

        await tester.tap(find.widgetWithText(TextButton, 'Volver'));
        await tester.pumpAndSettle();
        expect(host.calls, isNot(contains('POST /v1/plans/plan_a/approve')));
        expect(find.widgetWithText(FilledButton, 'Aprobar'), findsOneWidget);
      },
    );

    testWidgets('«Aprobar igualmente» sends the approval after the warning', (
      tester,
    ) async {
      final host = ScriptedTransport({
        'GET /v1/plans': _list([
          _plan('plan_a', 'proposed', touches: ['network']),
        ]),
        'POST /v1/plans/plan_a/approve': _answer(_plan('plan_a', 'approved')),
      });
      await pumpPlans(tester, host);

      await tester.tap(find.widgetWithText(FilledButton, 'Aprobar'));
      await tester.pumpAndSettle();
      await tester.tap(find.widgetWithText(FilledButton, 'Aprobar igualmente'));
      await tester.pumpAndSettle();

      expect(host.calls, contains('POST /v1/plans/plan_a/approve'));
    });

    testWidgets('rejecting asks first, and «Rechazar plan» sends it', (
      tester,
    ) async {
      final host = ScriptedTransport({
        'GET /v1/plans': _list([_plan('plan_a', 'proposed')]),
        'POST /v1/plans/plan_a/reject': _answer(_plan('plan_a', 'rejected')),
      });
      await pumpPlans(tester, host);

      await tester.tap(find.widgetWithText(TextButton, 'Rechazar'));
      await tester.pumpAndSettle();
      expect(find.text('¿Rechazar este plan?'), findsOneWidget);
      expect(host.calls, isNot(contains('POST /v1/plans/plan_a/reject')));

      await tester.tap(find.widgetWithText(TextButton, 'Rechazar plan'));
      await tester.pumpAndSettle();
      expect(host.calls, contains('POST /v1/plans/plan_a/reject'));
    });

    testWidgets('«Cambiar…» asks for the note with its counter, and sends it', (
      tester,
    ) async {
      final host = ScriptedTransport({
        'GET /v1/plans': _list([_plan('plan_a', 'proposed')]),
        'POST /v1/plans/plan_a/request_changes': _answer(
          _plan('plan_a', 'changes_requested'),
        ),
      });
      await pumpPlans(tester, host);

      await tester.tap(find.widgetWithText(OutlinedButton, 'Cambiar…'));
      await tester.pumpAndSettle();
      expect(find.text('¿Qué cambiarías?'), findsOneWidget);
      expect(find.text('0/300'), findsOneWidget);

      await tester.tap(find.widgetWithText(FilledButton, 'Enviar cambios'));
      await tester.pumpAndSettle();
      expect(find.text('Escribe qué cambiarías del plan.'), findsOneWidget);
      expect(
        host.calls,
        isNot(contains('POST /v1/plans/plan_a/request_changes')),
      );

      await tester.enterText(find.byType(TextField), '  Cambia el paso 2  ');
      await tester.pumpAndSettle();
      expect(find.text('20/300'), findsOneWidget);
      await tester.tap(find.widgetWithText(FilledButton, 'Enviar cambios'));
      await tester.pumpAndSettle();

      expect(host.bodies['POST /v1/plans/plan_a/request_changes'], [
        {'note': 'Cambia el paso 2'},
      ]);
      expect(find.text('Pediste cambios'), findsOneWidget);
    });
  });

  group('without APPROVE', () {
    testWidgets(
      'a refused decision locks the answers and says how to grant it',
      (tester) async {
        final host = ScriptedTransport({
          'GET /v1/plans': _list([_plan('plan_a', 'proposed')]),
        });
        await pumpPlans(tester, host);
        host.failNext(
          'POST /v1/plans/plan_a/approve',
          const HostError(403, '{}'),
        );

        await tester.tap(find.widgetWithText(FilledButton, 'Aprobar'));
        await tester.pumpAndSettle();

        expect(find.text(planPermissionText), findsOneWidget);
        expect(
          tester
              .widget<FilledButton>(
                find.widgetWithText(FilledButton, 'Aprobar'),
              )
              .onPressed,
          isNull,
        );
      },
    );
  });

  group('the layout', () {
    testWidgets(
      'answers stay 44 dp tall and nothing overflows at 360 dp and 1.3×',
      (tester) async {
        await pumpPlans(
          tester,
          ScriptedTransport({
            'GET /v1/plans': _list([
              _plan('plan_a', 'proposed', touches: ['files', 'mail', 'tools']),
            ]),
          }),
          width: 360,
          textScale: 1.3,
        );

        expect(
          tester.getSize(find.widgetWithText(FilledButton, 'Aprobar')).height,
          greaterThanOrEqualTo(44),
        );
        expect(
          tester
              .getSize(find.widgetWithText(OutlinedButton, 'Cambiar…'))
              .height,
          greaterThanOrEqualTo(44),
        );
        expect(
          tester.getSize(find.widgetWithText(TextButton, 'Rechazar')).height,
          greaterThanOrEqualTo(44),
        );
      },
    );
  });

  group('the header of Tu equipo', () {
    testWidgets(
      'a pending plan with nobody waiting says so in the team header',
      (tester) async {
        final teamHost = FakeTeamHost();
        final hires = HiresController(transport: teamHost);
        await hires.load();
        addTearDown(hires.dispose);
        final summary = TeamSummaryController(
          api: TeamSummaryApi(
            ScriptedTransport({
              'GET /v1/team/summary': jsonEncode({'ok': true, 'members': []}),
            }),
          ),
          interval: const Duration(hours: 1),
        );
        addTearDown(summary.dispose);
        await summary.refresh();
        final plans = PlansController(
          api: PlansApi(
            ScriptedTransport({
              'GET /v1/plans': _list([_plan('plan_a', 'proposed')]),
            }),
          ),
          interval: const Duration(hours: 1),
        );
        addTearDown(plans.dispose);
        await plans.refresh();
        tester.view.physicalSize = const Size(390 * 3, 2400 * 3);
        tester.view.devicePixelRatio = 3;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        await tester.pumpWidget(
          MaterialApp(
            theme: buildTheme(),
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

        expect(find.text('1 plan espera tu visto bueno'), findsOneWidget);
      },
    );
  });
}
