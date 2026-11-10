import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/recipes_api.dart';
import 'package:excubia/core/state/hires_controller.dart';
import 'package:excubia/core/state/recipes_controller.dart';
import 'package:excubia/src/rust/frb_generated.dart';
import 'package:excubia/ui/team/hire_catalog_screen.dart';
import 'package:excubia/ui/team/hires_section.dart';
import 'package:excubia/ui/theme.dart';
import 'package:excubia/ui/widgets/agent_avatar.dart';

import 'support/fake_team_host.dart';

const _empty = 'Todavía no hay nadie contratado en tu equipo.';
const _inviting =
    'Todavía no has contratado a nadie. Pulsa «Contratar» para elegir por área.';
const _dismissQuestion = '¿Despedir a Bob? Dejará de formar parte de tu equipo';
const _nameRule =
    'El nombre debe tener entre 1 y 40 caracteres, sin caracteres de control.';

Future<HiresController> pumpTeam(
  WidgetTester tester,
  FakeTeamHost host, {
  TextScaler textScaler = TextScaler.noScaling,
}) async {
  final hires = HiresController(transport: host);
  await hires.load();
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      home: MediaQuery(
        data: MediaQueryData(disableAnimations: true, textScaler: textScaler),
        child: Scaffold(
          body: ListView(
            children: [
              TeamHiresSection(
                hires: hires,
                recipes: RecipesController(
                  api: RecipesApi(host),
                  conversationId: () => null,
                ),
              ),
            ],
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return hires;
}

Finder catalogScreen() => find.byType(HireCatalogScreen);

/// The «Contratar» buttons inside the catalogue, one per role not yet hired.
Finder catalogHireButtons() => find.descendant(
  of: catalogScreen(),
  matching: find.widgetWithText(FilledButton, 'Contratar'),
);

void main() {
  setUpAll(RustLib.init);

  test('the team count says how many are hired and how many are active', () {
    expect(teamCountLabel(hired: 12, active: 0), '12 en el equipo · 0 activos');
    expect(teamCountLabel(hired: 1, active: 1), '1 en el equipo · 1 activo');
  });

  group('the team section', () {
    testWidgets('an empty team without can_hire says so and offers nothing', (
      tester,
    ) async {
      await pumpTeam(tester, FakeTeamHost(canHire: false));

      expect(find.text('CONTRATADOS'), findsOneWidget);
      expect(find.text(_empty), findsOneWidget);
      expect(find.text('Contratar'), findsNothing);
    });

    testWidgets('an empty team with can_hire invites to contract', (
      tester,
    ) async {
      await pumpTeam(tester, FakeTeamHost());

      expect(find.text(_inviting), findsOneWidget);
      expect(find.widgetWithText(FilledButton, 'Contratar'), findsOneWidget);
    });

    testWidgets('hires show their name, title and area, grouped by area', (
      tester,
    ) async {
      await pumpTeam(tester, FakeTeamHost(hires: [bobHire()]));

      expect(find.text('Bob'), findsOneWidget);
      expect(find.text('Desarrollador'), findsOneWidget);
      expect(find.text('Programación'), findsOneWidget);
    });

    testWidgets(
      'a hire without a character shows its initial, not a face by name',
      (tester) async {
        await pumpTeam(tester, FakeTeamHost(hires: [bobHire(avatar: null)]));

        final avatar = tester.widget<AgentAvatar>(find.byType(AgentAvatar));
        expect(avatar.characterId, isNull);
        expect(avatar.faceFromName, isFalse);
        expect(find.text('B'), findsOneWidget);
      },
    );

    testWidgets('without can_hire a person cannot be opened or changed', (
      tester,
    ) async {
      await pumpTeam(tester, FakeTeamHost(canHire: false, hires: [bobHire()]));

      await tester.tap(find.text('Bob'));
      await tester.pumpAndSettle();

      expect(find.text('Renombrar'), findsNothing);
      expect(find.text('Despedir'), findsNothing);
      expect(find.widgetWithText(FilledButton, 'Contratar'), findsNothing);
    });
  });

  group('actions on a contracted person', () {
    testWidgets('the three actions are offered with can_hire', (tester) async {
      await pumpTeam(tester, FakeTeamHost(hires: [bobHire()]));

      await tester.tap(find.text('Bob'));
      await tester.pumpAndSettle();

      expect(find.text('Renombrar'), findsOneWidget);
      expect(find.text('Cambiar avatar'), findsOneWidget);
      expect(find.text('Despedir'), findsOneWidget);
    });

    testWidgets('dismissing asks first, and cancelling keeps the person', (
      tester,
    ) async {
      final host = FakeTeamHost(hires: [bobHire()]);
      await pumpTeam(tester, host);

      await tester.tap(find.text('Bob'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Despedir'));
      await tester.pumpAndSettle();
      expect(find.text(_dismissQuestion), findsOneWidget);

      await tester.tap(find.text('Cancelar'));
      await tester.pumpAndSettle();

      expect(host.posts, isEmpty);
      expect(find.text('Bob'), findsOneWidget);
    });

    testWidgets('confirming dismissal sends it and the list drops the person', (
      tester,
    ) async {
      final host = FakeTeamHost(hires: [bobHire()]);
      await pumpTeam(tester, host);

      await tester.tap(find.text('Bob'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Despedir'));
      await tester.pumpAndSettle();
      await tester.tap(find.widgetWithText(FilledButton, 'Despedir'));
      await tester.pumpAndSettle();

      expect(host.calls, contains('POST /v1/hires/hire_1/dismiss'));
      expect(find.text('Bob'), findsNothing);
      expect(find.text(_empty), findsNothing);
      expect(find.text(_inviting), findsOneWidget);
    });

    testWidgets(
      'the avatar picker offers the eight characters and the initial, and marks the current one',
      (tester) async {
        await pumpTeam(tester, FakeTeamHost(hires: [bobHire()]));

        await tester.tap(find.text('Bob'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Cambiar avatar'));
        await tester.pumpAndSettle();

        for (final name in [
          'Bob',
          'Conan',
          'Dani',
          'Ernesto',
          'Hugo',
          'Ines',
          'Nicole',
          'Sami',
        ]) {
          expect(find.text(name), findsWidgets);
        }
        expect(find.text('Sin personaje (inicial)'), findsOneWidget);
        expect(
          find.byIcon(Icons.check),
          findsOneWidget,
          reason: 'only the current one',
        );
      },
    );

    testWidgets('choosing a character sends its id and shows that face', (
      tester,
    ) async {
      final host = FakeTeamHost(hires: [bobHire()]);
      await pumpTeam(tester, host);

      await tester.tap(find.text('Bob'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Cambiar avatar'));
      await tester.pumpAndSettle();
      await tester.ensureVisible(find.text('Ines'));
      await tester.tap(find.text('Ines'));
      await tester.pumpAndSettle();

      expect(host.posts.last, {'avatar': 'ines'});
      expect(
        find.byWidgetPredicate(
          (w) => w is AgentAvatar && w.characterId == 'ines',
        ),
        findsOneWidget,
      );
    });

    testWidgets('renaming keeps the character of the hire', (tester) async {
      final host = FakeTeamHost(hires: [bobHire()]);
      await pumpTeam(tester, host);

      await tester.tap(find.text('Bob'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Renombrar'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField), 'Roberto');
      await tester.tap(find.widgetWithText(FilledButton, 'Guardar'));
      await tester.pumpAndSettle();

      expect(host.posts.last, {'name': 'Roberto'});
      expect(find.text('Roberto'), findsOneWidget);
      expect(
        find.byWidgetPredicate(
          (w) => w is AgentAvatar && w.characterId == 'bob',
        ),
        findsOneWidget,
        reason: 'the face does not depend on the name',
      );
    });

    testWidgets(
      'a name outside the rules is said in the dialog and nothing is sent',
      (tester) async {
        final host = FakeTeamHost(hires: [bobHire()]);
        await pumpTeam(tester, host);

        await tester.tap(find.text('Bob'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Renombrar'));
        await tester.pumpAndSettle();
        await tester.enterText(find.byType(TextField), '   ');
        await tester.tap(find.widgetWithText(FilledButton, 'Guardar'));
        await tester.pumpAndSettle();

        expect(find.text(_nameRule), findsOneWidget);
        expect(host.posts, isEmpty);
      },
    );

    testWidgets('the name rule wraps onto several lines at a large text size', (
      tester,
    ) async {
      await pumpTeam(
        tester,
        FakeTeamHost(hires: [bobHire()]),
        textScaler: const TextScaler.linear(2),
      );

      await tester.tap(find.text('Bob'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Renombrar'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField), '   ');
      await tester.tap(find.widgetWithText(FilledButton, 'Guardar'));
      await tester.pumpAndSettle();

      expect(tester.takeException(), isNull);
      final rule = find.text(_nameRule);
      expect(tester.getSize(rule).height, greaterThan(45), reason: 'two lines');
    });
  });

  group('the catalogue', () {
    testWidgets(
      'Contratar opens the catalogue by area and marks who is hired',
      (tester) async {
        await pumpTeam(tester, FakeTeamHost(hires: [bobHire()]));

        await tester.tap(find.widgetWithText(FilledButton, 'Contratar'));
        await tester.pumpAndSettle();

        expect(catalogScreen(), findsOneWidget);
        expect(find.text('Vídeo y postproducción'), findsOneWidget);
        expect(find.text('Contratado'), findsOneWidget, reason: 'only Bob');
        expect(catalogHireButtons(), findsNWidgets(3));
      },
    );

    testWidgets('hiring from the catalogue confirms and sends the role', (
      tester,
    ) async {
      final host = FakeTeamHost();
      await pumpTeam(tester, host);

      await tester.tap(find.widgetWithText(FilledButton, 'Contratar'));
      await tester.pumpAndSettle();
      await tester.tap(catalogHireButtons().first);
      await tester.pumpAndSettle();

      expect(host.posts.last, {'role': 'vera'});
      expect(find.text('Contratado: Vera'), findsOneWidget);
    });

    testWidgets('a full area says so in words', (tester) async {
      final host = FakeTeamHost()..areaFull = true;
      await pumpTeam(tester, host);

      await tester.tap(find.widgetWithText(FilledButton, 'Contratar'));
      await tester.pumpAndSettle();
      await tester.tap(catalogHireButtons().first);
      await tester.pumpAndSettle();

      expect(find.text('Esa área ya tiene 8 agentes.'), findsOneWidget);
      expect(host.posts, hasLength(1), reason: 'one request, no silent retry');
    });
  });
}
