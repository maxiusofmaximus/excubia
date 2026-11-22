import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/recipes_api.dart';
import 'package:excubia/core/state/recipes_controller.dart';
import 'package:excubia/ui/team/hire_recipes.dart';
import 'package:excubia/ui/theme.dart';

import '../support/scripted_transport.dart';

const _listKey = 'GET /v1/recipes?role=audio-editor';
const _listPath = 'GET /v1/recipes';
const _runKey = 'POST /v1/recipes/rcp_1/run';

const _list =
    '{"ok":true,"recipes":[{"id":"rcp_1","role_id":"audio-editor",'
    '"title":"Resumen","body":"Haz un resumen.","uses":2,'
    '"created_at":"2026-11-01T10:00:00.000Z","updated_at":"2026-11-02T10:00:00.000Z"}]}';

const _run = '{"ok":true,"delegation_id":"del_1","status":"queued","depth":1}';

String _coded(String code) => jsonEncode({
  'detail': {'code': code},
});

Future<void> _pump(
  WidgetTester tester,
  RecipesController recipes, {
  TextScaler scale = TextScaler.noScaling,
  VoidCallback? onHire,
}) async {
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      home: MediaQuery(
        data: MediaQueryData(disableAnimations: true, textScaler: scale),
        child: Scaffold(
          body: SingleChildScrollView(
            child: HireRecipes(
              recipes: recipes,
              personName: 'Ines',
              onHire: onHire,
            ),
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

RecipesController _opened(ScriptedTransport host) =>
    RecipesController(api: RecipesApi(host), conversationId: () => 'conv_1');

ScriptedTransport _host() =>
    ScriptedTransport({_listKey: _list, _runKey: _run});

Finder _repeat() => find.widgetWithText(FilledButton, 'Repetir');

void main() {
  group('the list', () {
    testWidgets(
      'shows each recipe with its use count and one line of its text',
      (tester) async {
        final recipes = _opened(_host());
        await recipes.open('audio-editor');

        await _pump(tester, recipes);

        expect(find.text('RECETAS'), findsOneWidget);
        expect(find.text('Resumen'), findsOneWidget);
        expect(find.text('usada 2 veces'), findsOneWidget);
        expect(find.text('Haz un resumen.'), findsOneWidget);
        expect(_repeat(), findsOneWidget);
        expect(find.widgetWithText(TextButton, 'Ver receta'), findsOneWidget);
      },
    );

    testWidgets('an empty list says where recipes come from', (tester) async {
      final recipes = _opened(
        ScriptedTransport({_listKey: '{"ok":true,"recipes":[]}'}),
      );
      await recipes.open('audio-editor');

      await _pump(tester, recipes);

      expect(find.text('Aún no hay recetas.'), findsOneWidget);
      expect(
        find.text('Guárdalas desde el escritorio cuando algo salga bien.'),
        findsOneWidget,
      );
      expect(_repeat(), findsNothing);
    });

    testWidgets('while the first read is out, a loading block is shown', (
      tester,
    ) async {
      await _pump(tester, _opened(_host()));

      expect(find.bySemanticsLabel('Cargando recetas'), findsOneWidget);
    });

    testWidgets('a failed read says it once, and Reintentar reads again', (
      tester,
    ) async {
      final host = _host()..failNext(_listPath, const HostError(500, '{}'));
      final recipes = _opened(host);
      await recipes.open('audio-editor');

      await _pump(tester, recipes);
      expect(
        find.text('No se pudieron leer las recetas. Inténtalo de nuevo.'),
        findsOneWidget,
      );

      await tester.tap(find.widgetWithText(TextButton, 'Reintentar'));
      await tester.pumpAndSettle();

      expect(find.text('Resumen'), findsOneWidget);
      expect(host.calls.where((call) => call == _listPath).length, 2);
    });
  });

  group('repeating', () {
    testWidgets('asks first, names the person, and Cancelar runs nothing', (
      tester,
    ) async {
      final host = _host();
      final recipes = _opened(host);
      await recipes.open('audio-editor');
      await _pump(tester, recipes);

      await tester.tap(_repeat());
      await tester.pumpAndSettle();
      expect(find.text('Se lanzará este encargo a Ines.'), findsOneWidget);

      await tester.tap(find.widgetWithText(TextButton, 'Cancelar'));
      await tester.pumpAndSettle();

      expect(host.calls.where((call) => call == _runKey), isEmpty);
    });

    testWidgets('confirming runs it and says Encargo lanzado', (tester) async {
      final host = _host();
      final recipes = _opened(host);
      await recipes.open('audio-editor');
      await _pump(tester, recipes);

      await tester.tap(_repeat());
      await tester.pumpAndSettle();
      await tester.tap(find.widgetWithText(FilledButton, 'Lanzar encargo'));
      await tester.pumpAndSettle();

      expect(find.text('Encargo lanzado'), findsOneWidget);
      expect(host.bodies[_runKey], [
        {'conversation_id': 'conv_1'},
      ]);
    });

    testWidgets(
      'a busy agent disables the button with a countdown, then frees it',
      (tester) async {
        final host = _host()
          ..failNext(
            _runKey,
            HostError(409, _coded('agent_busy'), retryAfter: 2),
          );
        final recipes = _opened(host);
        await recipes.open('audio-editor');
        await _pump(tester, recipes);

        await tester.tap(_repeat());
        await tester.pumpAndSettle();
        await tester.tap(find.widgetWithText(FilledButton, 'Lanzar encargo'));
        await tester.pump();

        expect(
          find.text('Ahora está ocupado; vuelve a intentarlo en 2 s.'),
          findsOneWidget,
        );
        expect(_repeat(), findsNothing);
        final waiting = tester.widget<FilledButton>(
          find.widgetWithText(FilledButton, 'Repetir en 2 s'),
        );
        expect(waiting.onPressed, isNull);

        await tester.pump(const Duration(seconds: 2));

        expect(tester.widget<FilledButton>(_repeat()).onPressed, isNotNull);
      },
    );

    testWidgets('without APPROVE the button is off and says why', (
      tester,
    ) async {
      final host = _host()..failNext(_runKey, const HostError(403, '{}'));
      final recipes = _opened(host);
      await recipes.open('audio-editor');
      await _pump(tester, recipes);

      await tester.tap(_repeat());
      await tester.pumpAndSettle();
      await tester.tap(find.widgetWithText(FilledButton, 'Lanzar encargo'));
      await tester.pumpAndSettle();

      expect(find.text(recipePermissionText), findsOneWidget);
      expect(tester.widget<FilledButton>(_repeat()).onPressed, isNull);
    });

    testWidgets('a missing hire says so and offers Contratar', (tester) async {
      var hired = false;
      final host = _host()
        ..failNext(_runKey, HostError(409, _coded('no_hire_for_role')));
      final recipes = _opened(host);
      await recipes.open('audio-editor');
      await _pump(tester, recipes, onHire: () => hired = true);

      await tester.tap(_repeat());
      await tester.pumpAndSettle();
      await tester.tap(find.widgetWithText(FilledButton, 'Lanzar encargo'));
      await tester.pumpAndSettle();
      await tester.tap(find.widgetWithText(TextButton, 'Contratar'));

      expect(hired, isTrue);
    });
  });

  group('Ver receta', () {
    testWidgets('shows the whole text in a sheet that cannot be edited', (
      tester,
    ) async {
      final recipes = _opened(_host());
      await recipes.open('audio-editor');
      await _pump(tester, recipes);

      await tester.tap(find.widgetWithText(TextButton, 'Ver receta'));
      await tester.pumpAndSettle();

      expect(find.text('Haz un resumen.'), findsWidgets);
      expect(find.byType(TextField), findsNothing);
      await tester.tap(find.widgetWithText(TextButton, 'Cerrar'));
      await tester.pumpAndSettle();
      expect(find.widgetWithText(TextButton, 'Cerrar'), findsNothing);
    });
  });

  testWidgets('fits 360 dp with the text at 1.3 times its size', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(360, 780);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final recipes = _opened(_host());
    await recipes.open('audio-editor');

    await _pump(tester, recipes, scale: const TextScaler.linear(1.3));

    expect(tester.takeException(), isNull);
    expect(find.text('Resumen'), findsOneWidget);
  });
}
