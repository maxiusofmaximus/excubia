import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/ui/palette/palette_controller.dart';
import 'package:excubia/ui/palette/palette_prefs.dart';
import 'package:excubia/ui/palette/palette_screen.dart';
import 'package:excubia/ui/palette/palette_scope.dart';
import 'package:excubia/ui/theme.dart';

import 'support/captures.dart';
import 'support/memory_store.dart';

final String? _evidenceDir = evidenceDir('APK7_EVIDENCE');

/// The button label has no font family, and the test font (Ahem) draws it as
/// solid boxes. The phone uses the system font, so the capture names Roboto.
ThemeData _readable(ThemeData theme) => theme.copyWith(
  filledButtonTheme: FilledButtonThemeData(
    style: theme.filledButtonTheme.style!.copyWith(
      textStyle: const WidgetStatePropertyAll(
        TextStyle(
          fontFamily: 'Roboto',
          fontWeight: FontWeight.w600,
          fontSize: 15,
        ),
      ),
    ),
  ),
);

Widget _app(
  GlobalKey key,
  PaletteController controller, {
  PaletteScreenMode mode = PaletteScreenMode.firstRun,
  double textScale = 1,
}) => RepaintBoundary(
  key: key,
  child: PaletteScope(
    controller: controller,
    child: ListenableBuilder(
      listenable: controller,
      builder: (context, _) => MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: _readable(buildTheme(controller.paletteFor(Brightness.dark))),
        home: Builder(
          builder: (context) => MediaQuery(
            data: MediaQuery.of(context)
                .copyWith(textScaler: TextScaler.linear(textScale)),
            child: PaletteScreen(mode: mode),
          ),
        ),
      ),
    ),
  ),
);

Future<void> _mount(WidgetTester tester, Widget app) async {
  tester.view.physicalSize = const Size(390, 844);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  // The captured theme is the dark one, so the system must read dark too.
  tester.binding.platformDispatcher.platformBrightnessTestValue =
      Brightness.dark;
  addTearDown(
    tester.binding.platformDispatcher.clearPlatformBrightnessTestValue,
  );
  await tester.pumpWidget(app);
  // The logo decodes on real I/O; without this wait the first capture misses it.
  await tester.runAsync(
    () => Future<void>.delayed(const Duration(milliseconds: 200)),
  );
  await tester.pumpAndSettle();
}

void main() {
  setUpAll(() async {
    if (_evidenceDir != null) await loadReadableFonts();
  });

  testWidgets(
    'capturas: Noche, Pergamino y Alto contraste',
    skip: _evidenceDir == null,
    (tester) async {
      final controller = await PaletteController.load(
        prefs: PalettePrefs(MemoryKeyValueStore()),
        hasPairing: false,
      );
      final key = GlobalKey();
      await _mount(tester, _app(key, controller));
      await saveCapture(tester, key, _evidenceDir, 'apk8-01-paleta-noche');

      await tester.tap(find.text('Pergamino'));
      await tester.pumpAndSettle();
      await saveCapture(tester, key, _evidenceDir, 'apk8-02-paleta-pergamino');

      await tester.tap(find.text('Alto contraste'));
      await tester.pumpAndSettle();
      await saveCapture(tester, key, _evidenceDir, 'apk8-03-paleta-contraste');
    },
  );

  testWidgets('captura: Apariencia en ajustes', skip: _evidenceDir == null, (
    tester,
  ) async {
    final controller = await PaletteController.load(
      prefs: PalettePrefs(MemoryKeyValueStore()),
      hasPairing: true,
    );
    final key = GlobalKey();
    await _mount(
      tester,
      _app(key, controller, mode: PaletteScreenMode.settings),
    );
    await saveCapture(tester, key, _evidenceDir, 'apk8-04-apariencia');
  });

  testWidgets(
    'captura: la pantalla con el texto al doble',
    skip: _evidenceDir == null,
    (tester) async {
      final controller = await PaletteController.load(
        prefs: PalettePrefs(MemoryKeyValueStore()),
        hasPairing: false,
      );
      final key = GlobalKey();
      await _mount(tester, _app(key, controller, textScale: 2));
      await saveCapture(tester, key, _evidenceDir, 'apk8-05-paleta-2x');
    },
  );
}
