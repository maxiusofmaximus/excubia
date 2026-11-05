import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/ui/palette/palette_controller.dart';
import 'package:herdr_notify/ui/palette/palette_prefs.dart';
import 'package:herdr_notify/ui/palette/palette_screen.dart';
import 'package:herdr_notify/ui/palette/palette_scope.dart';
import 'package:herdr_notify/ui/theme.dart';

import 'support/memory_store.dart';

void main() {
  late MemoryKeyValueStore values;
  late PalettePrefs prefs;
  late PaletteController controller;

  setUp(() async {
    values = MemoryKeyValueStore();
    prefs = PalettePrefs(values);
    controller = await PaletteController.load(prefs: prefs, hasPairing: false);
  });

  Future<void> mount(
    WidgetTester tester, {
    PaletteScreenMode mode = PaletteScreenMode.firstRun,
    Brightness system = Brightness.dark,
    double textScale = 1,
    VoidCallback? onContinue,
  }) async {
    tester.view.physicalSize = const Size(390, 844);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    tester.binding.platformDispatcher.platformBrightnessTestValue = system;
    addTearDown(
      tester.binding.platformDispatcher.clearPlatformBrightnessTestValue,
    );
    await tester.pumpWidget(
      PaletteScope(
        controller: controller,
        child: ListenableBuilder(
          listenable: controller,
          builder: (context, _) => MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: buildTheme(controller.paletteFor(system)),
            home: Builder(
              builder: (context) => MediaQuery(
                data: MediaQuery.of(context)
                    .copyWith(textScaler: TextScaler.linear(textScale)),
                child: PaletteScreen(mode: mode, onContinue: onContinue),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  Semantics card(WidgetTester tester, String id) =>
      tester.widget<Semantics>(find.byKey(ValueKey('palette-$id')));

  Color pageColor(WidgetTester tester) =>
      tester.widget<Scaffold>(find.byType(Scaffold)).backgroundColor!;

  testWidgets('muestra las seis paletas, el interruptor y Continuar', (
    tester,
  ) async {
    await mount(tester);

    expect(find.text('Elige tu paleta'), findsOneWidget);
    expect(find.text('La puedes cambiar luego en Ajustes.'), findsOneWidget);
    for (final palette in HerdrPalettes.all) {
      expect(find.text(palette.name), findsOneWidget);
    }
    expect(find.text('Seguir el tema del sistema'), findsOneWidget);
    expect(find.text('Continuar'), findsOneWidget);
    expect(card(tester, 'noche').properties.selected, isTrue);
    expect(card(tester, 'carbon').properties.selected, isFalse);
  });

  testWidgets('elegir una tarjeta cambia el tema al instante', (tester) async {
    await mount(tester);

    await tester.tap(find.text('Carbón'));
    await tester.pumpAndSettle();

    expect(controller.paletteFor(Brightness.dark).id, 'carbon');
    expect(pageColor(tester), HerdrColors.carbon.bg);
    expect(card(tester, 'carbon').properties.selected, isTrue);
    expect(card(tester, 'noche').properties.selected, isFalse);
  });

  testWidgets('seguir el sistema aplica Pergamino cuando el sistema es claro', (
    tester,
  ) async {
    await mount(tester, system: Brightness.light);
    expect(pageColor(tester), HerdrColors.noche.bg);

    await tester.tap(find.text('Seguir el tema del sistema'));
    await tester.pumpAndSettle();

    expect(controller.followSystem, isTrue);
    expect(pageColor(tester), HerdrColors.pergamino.bg);
    expect(card(tester, 'pergamino').properties.selected, isTrue);

    await tester.tap(find.text('Medianoche'));
    await tester.pumpAndSettle();

    expect(controller.followSystem, isFalse);
    expect(pageColor(tester), HerdrColors.medianoche.bg);
  });

  testWidgets('Continuar guarda la elección', (tester) async {
    var continued = 0;
    await mount(tester, onContinue: () => continued++);

    await tester.tap(find.text('Carbón'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Continuar'));
    await tester.pumpAndSettle();

    expect(continued, 1);
    expect(controller.needsChoice, isFalse);
    expect((await prefs.load())?.id, 'carbon');
  });

  testWidgets('sin desbordes a 1,3x y 2,0x', (tester) async {
    for (final scale in [1.3, 2.0]) {
      await mount(tester, textScale: scale);

      expect(tester.takeException(), isNull);
      expect(find.text('Continuar'), findsOneWidget);
      expect(find.text('Seguir el tema del sistema'), findsOneWidget);
    }
  });

  testWidgets('en ajustes no hay Continuar y cada cambio se guarda', (
    tester,
  ) async {
    await mount(tester, mode: PaletteScreenMode.settings);

    expect(find.widgetWithText(AppBar, 'Apariencia'), findsOneWidget);
    expect(find.text('Continuar'), findsNothing);

    await tester.tap(find.text('Papel'));
    await tester.pumpAndSettle();

    expect((await prefs.load())?.id, 'papel');
  });
}
