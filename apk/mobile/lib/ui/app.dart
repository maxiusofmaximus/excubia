import 'package:flutter/material.dart';

import '../core/app_info.dart';
import '../core/storage/pairing_store.dart';
import 'home_shell.dart';
import 'onboarding/onboarding_gate.dart';
import 'palette/palette_controller.dart';
import 'palette/palette_prefs.dart';
import 'palette/palette_screen.dart';
import 'palette/palette_scope.dart';
import 'pairing_screen.dart';
import 'theme.dart';

/// The app draws edge to edge, and the lists with a fixed `padding` ignore the bar of
/// buttons at the bottom: their last row stayed under it. Every screen ends above it.
Widget withSystemBarsClear(BuildContext context, Widget? child) => ColoredBox(
  color: context.colors.bg,
  child: SafeArea(
    top: false,
    left: false,
    right: false,
    child: child ?? const SizedBox.shrink(),
  ),
);

class ExcubiaApp extends StatefulWidget {
  const ExcubiaApp({
    super.key,
    required this.store,
    required this.palettePrefs,
  });

  final PairingStore store;
  final PalettePrefs palettePrefs;

  @override
  State<ExcubiaApp> createState() => _ExcubiaAppState();
}

class _ExcubiaAppState extends State<ExcubiaApp>
    with WidgetsBindingObserver {
  Pairing? _pairing;
  PaletteController? _palette;
  Brightness _systemBrightness =
      WidgetsBinding.instance.platformDispatcher.platformBrightness;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _boot();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  void didChangePlatformBrightness() {
    setState(
      () => _systemBrightness =
          WidgetsBinding.instance.platformDispatcher.platformBrightness,
    );
  }

  Future<void> _boot() async {
    final pairing = await widget.store.load();
    final palette = await PaletteController.load(
      prefs: widget.palettePrefs,
      hasPairing: pairing.isComplete,
    );
    if (!mounted) return;
    setState(() {
      _pairing = pairing;
      _palette = palette;
    });
  }

  void _onPaired(Pairing pairing) => setState(() => _pairing = pairing);

  Future<void> _onUnpaired() async {
    await widget.store.clear();
    if (!mounted) return;
    setState(() => _pairing = Pairing.empty);
  }

  @override
  Widget build(BuildContext context) {
    final palette = _palette;
    final pairing = _pairing;
    if (palette == null || pairing == null) {
      return MaterialApp(
        title: appName,
        debugShowCheckedModeBanner: false,
        theme: buildTheme(),
        builder: withSystemBarsClear,
        home: Builder(
          builder: (context) => ColoredBox(color: context.colors.bg),
        ),
      );
    }
    return PaletteScope(
      controller: palette,
      child: ListenableBuilder(
        listenable: palette,
        builder: (context, _) => MaterialApp(
          title: appName,
          debugShowCheckedModeBanner: false,
          theme: buildTheme(palette.paletteFor(_systemBrightness)),
          builder: withSystemBarsClear,
          home: palette.needsChoice
              ? const PaletteScreen(mode: PaletteScreenMode.firstRun)
              : pairing.isComplete
              ? OnboardingGate(
                  key: ValueKey(pairing.deviceId),
                  pairing: pairing,
                  home: HomeShell(pairing: pairing, onUnpaired: _onUnpaired),
                )
              : PairingScreen(store: widget.store, onPaired: _onPaired),
        ),
      ),
    );
  }
}
