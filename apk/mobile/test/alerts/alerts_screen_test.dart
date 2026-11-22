import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/alerts_api.dart';
import 'package:excubia/ui/alerts/alerts_screen.dart';
import 'package:excubia/ui/theme.dart';

import '../alerts/alert_fakes.dart';
import '../support/scripted_transport.dart';

void main() {
  late FakeAlertPlatform platform;

  setUp(() => platform = FakeAlertPlatform());

  Future<void> pumpScreen(WidgetTester tester) async {
    // The list is lazy: a tall surface builds every row the finders below look for.
    tester.view.physicalSize = const Size(800, 8000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: AlertsScreen(
          rules: AlertsApi(ScriptedTransport({})),
          alerts: platform,
        ),
      ),
    );
    await tester.pump();
    await tester.pump();
  }

  testWidgets('the screen is titled Avisos and lists what the phone needs', (tester) async {
    await pumpScreen(tester);

    expect(find.text('Avisos'), findsOneWidget);
    expect(find.text('Lo que necesita tu móvil'), findsOneWidget);
    expect(find.text('Inicio automático'), findsOneWidget);
  });

  testWidgets('no ntfy, distributor or layer text is left on the screen', (tester) async {
    await pumpScreen(tester);

    expect(find.textContaining('ntfy'), findsNothing);
    expect(find.textContaining('Capa activa'), findsNothing);
    expect(find.textContaining('Distribuidor'), findsNothing);
  });
}
