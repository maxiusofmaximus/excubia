import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/state/onboarding_controller.dart';
import 'package:herdr_notify/core/storage/pairing_store.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/onboarding/onboarding_gate.dart';
import 'package:herdr_notify/ui/onboarding/onboarding_screen.dart';
import 'package:herdr_notify/ui/theme.dart';

import 'support/fake_onboarding_host.dart';

const _pairing = Pairing(
  baseUrl: 'https://host',
  deviceId: 'dev',
  privateKey: 'key',
);
const _title = '¿Para qué lo vas a usar?';

/// Animations off, so the avatar of Alfred does not keep the test waiting.
Widget app(Widget home) => MaterialApp(
  theme: buildTheme(),
  home: MediaQuery(
    data: const MediaQueryData(disableAnimations: true),
    child: home,
  ),
);

Future<void> pumpScreen(
  WidgetTester tester,
  FakeOnboardingHost host, {
  VoidCallback? onDone,
}) async {
  final controller = OnboardingController(transport: host);
  await tester.pumpWidget(
    app(OnboardingScreen(controller: controller, onDone: onDone ?? () {})),
  );
  await tester.pumpAndSettle();
}

Future<void> tapText(WidgetTester tester, String text) async {
  await tester.ensureVisible(find.text(text));
  await tester.tap(find.text(text));
  await tester.pumpAndSettle();
}

void main() {
  setUpAll(RustLib.init);

  group('entry gate', () {
    testWidgets('a pending onboarding with APPROVE shows the assistant first', (
      tester,
    ) async {
      final host = standardHost(onboarding: onboardingPendingCanHireJson);
      await tester.pumpWidget(
        app(
          OnboardingGate(
            pairing: _pairing,
            home: const Text('inicio'),
            transport: host,
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text(_title), findsOneWidget);
      expect(find.text('inicio'), findsNothing);
    });

    testWidgets('a done onboarding goes straight to the app', (tester) async {
      final host = standardHost(onboarding: onboardingDoneJson);
      await tester.pumpWidget(
        app(
          OnboardingGate(
            pairing: _pairing,
            home: const Text('inicio'),
            transport: host,
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('inicio'), findsOneWidget);
      expect(find.text(_title), findsNothing);
    });

    testWidgets('without APPROVE the app opens as it does today', (
      tester,
    ) async {
      final host = standardHost(onboarding: onboardingPendingNoHireJson);
      await tester.pumpWidget(
        app(
          OnboardingGate(
            pairing: _pairing,
            home: const Text('inicio'),
            transport: host,
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('inicio'), findsOneWidget);
      expect(find.text(_title), findsNothing);
      expect(find.textContaining('permiso'), findsNothing);
      expect(host.posts, isEmpty, reason: 'the gate never writes');
    });

    testWidgets('«Saltar» closes the assistant into the app', (tester) async {
      final host = standardHost(onboarding: onboardingPendingCanHireJson);
      await tester.pumpWidget(
        app(
          OnboardingGate(
            pairing: _pairing,
            home: const Text('inicio'),
            transport: host,
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tapText(tester, 'Saltar');

      expect(host.posts.last, {'skip': true});
      expect(find.text('inicio'), findsOneWidget);
    });
  });

  group('steps', () {
    testWidgets('the team is contracted only by «Contratar equipo»', (
      tester,
    ) async {
      final host = standardHost();
      await pumpScreen(tester, host);

      await tapText(tester, 'Continuar');
      expect(find.text('¿En qué trabajas?'), findsOneWidget);
      expect(_footerButton(tester).onPressed, isNull);

      await tapText(tester, 'Vídeo y postproducción');
      await tapText(tester, 'Continuar');
      expect(find.text('¿Cómo lo vas a usar?'), findsOneWidget);

      await tapText(tester, 'Continuar');
      expect(find.text('Este es tu equipo'), findsOneWidget);
      expect(
        find.text('Quita o añade personas; Alfred siempre está.'),
        findsOneWidget,
      );
      expect(_alfredRow(tester).value, isTrue);
      expect(_alfredRow(tester).onChanged, isNull);
      expect(host.posts, isEmpty);

      await tapText(tester, 'Contratar equipo');
      expect(host.posts, [
        {
          'areas': ['video'],
          'level': 'profesional',
          'roles': ['vera', 'jade'],
        },
      ]);
      expect(
        find.text(
          'He preparado 2 agentes para Vídeo y postproducción. Puedes ajustarlos en cualquier momento.',
        ),
        findsOneWidget,
      );
    });

    testWidgets('«Contratar equipo» stays disabled with nobody selected', (
      tester,
    ) async {
      final host = standardHost();
      await pumpScreen(tester, host);

      await tapText(tester, 'Continuar');
      await tapText(tester, 'Programación');
      await tapText(tester, 'Continuar');
      await tapText(tester, 'Continuar');
      await tapText(tester, 'Bob');

      expect(_roleRow(tester, 'Bob').value, isFalse);
      expect(_footerButton(tester, 'Contratar equipo').onPressed, isNull);
      expect(host.posts, isEmpty);
    });

    testWidgets('a ninth person in one area shows the limit and adds nothing', (
      tester,
    ) async {
      final host = standardHost(catalog: nineRolesCatalogJson);
      await pumpScreen(tester, host);

      await tapText(tester, 'Continuar');
      await tapText(tester, 'Vídeo y postproducción');
      await tapText(tester, 'Continuar');
      await tapText(tester, 'Equipo');
      await tapText(tester, 'Continuar');

      expect(find.text('8/8'), findsOneWidget);
      await tapText(tester, 'R9');

      expect(find.text('Máximo 8 por área'), findsOneWidget);
      expect(_roleRow(tester, 'R9').value, isFalse);
      expect(host.posts, isEmpty);
    });

    testWidgets('Alfred is the CTO, not an orchestrator', (tester) async {
      await pumpScreen(tester, standardHost());

      await tapText(tester, 'Continuar');
      await tapText(tester, 'Programación');
      await tapText(tester, 'Continuar');
      await tapText(tester, 'Continuar');

      expect(find.text('Director técnico'), findsOneWidget);
      expect(find.text('Orquestador del equipo'), findsNothing);
    });

    testWidgets('the last area can be scrolled clear of «Continuar»', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(400, 480);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      await pumpScreen(tester, standardHost());
      await tapText(tester, 'Continuar');

      await tester.drag(
        find.byType(SingleChildScrollView),
        const Offset(0, -1000),
      );
      await tester.pumpAndSettle();

      final last = tester.getRect(find.text('Otras'));
      final button = tester.getRect(
        find.widgetWithText(FilledButton, 'Continuar'),
      );
      expect(last.bottom, lessThanOrEqualTo(button.top));
    });

    testWidgets('a failed catalogue read offers a retry on the areas step', (
      tester,
    ) async {
      final host = standardHost()
        ..answers['GET /v1/roles'] = const HostError(0, '');
      await pumpScreen(tester, host);

      await tapText(tester, 'Continuar');

      expect(find.text('Reintentar'), findsOneWidget);
      expect(find.text('Continuar'), findsNothing);
    });

    testWidgets('«Saltar» on the welcome step skips without contracting', (
      tester,
    ) async {
      final host = standardHost();
      var done = false;
      await pumpScreen(tester, host, onDone: () => done = true);

      await tapText(tester, 'Saltar');

      expect(done, isTrue);
      expect(host.posts, [
        {'skip': true},
      ]);
    });

    testWidgets('«Atrás» returns to the previous step', (tester) async {
      await pumpScreen(tester, standardHost());

      await tapText(tester, 'Continuar');
      await tapText(tester, 'Atrás');

      expect(find.text(_title), findsOneWidget);
    });
  });
}

FilledButton _footerButton(WidgetTester tester, [String label = 'Continuar']) =>
    tester.widget<FilledButton>(
      find.ancestor(of: find.text(label), matching: find.byType(FilledButton)),
    );

CheckboxListTile _alfredRow(WidgetTester tester) => tester
    .widget<CheckboxListTile>(find.widgetWithText(CheckboxListTile, 'Alfred'));

CheckboxListTile _roleRow(WidgetTester tester, String name) => tester
    .widget<CheckboxListTile>(find.widgetWithText(CheckboxListTile, name));
