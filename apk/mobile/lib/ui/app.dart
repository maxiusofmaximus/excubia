import 'package:flutter/material.dart';

import '../core/app_info.dart';
import '../core/storage/pairing_store.dart';
import 'home_shell.dart';
import 'onboarding/onboarding_gate.dart';
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

class HerdrNotifyApp extends StatefulWidget {
  const HerdrNotifyApp({super.key, required this.store});

  final PairingStore store;

  @override
  State<HerdrNotifyApp> createState() => _HerdrNotifyAppState();
}

class _HerdrNotifyAppState extends State<HerdrNotifyApp> {
  Pairing? _pairing;
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    _boot();
  }

  Future<void> _boot() async {
    final pairing = await widget.store.load();
    if (!mounted) return;
    setState(() {
      _pairing = pairing;
      _loading = false;
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
    final pairing = _pairing;
    return MaterialApp(
      title: appName,
      debugShowCheckedModeBanner: false,
      theme: buildTheme(Brightness.light),
      darkTheme: buildTheme(),
      themeMode: ThemeMode.system,
      builder: withSystemBarsClear,
      home: _loading || pairing == null
          ? Builder(builder: (context) => ColoredBox(color: context.colors.bg))
          : pairing.isComplete
          ? OnboardingGate(
              key: ValueKey(pairing.deviceId),
              pairing: pairing,
              home: HomeShell(pairing: pairing, onUnpaired: _onUnpaired),
            )
          : PairingScreen(store: widget.store, onPaired: _onPaired),
    );
  }
}
