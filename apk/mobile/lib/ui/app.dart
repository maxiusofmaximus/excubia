import 'package:flutter/material.dart';

import '../core/app_info.dart';
import '../core/migration/legacy_migration.dart';
import '../core/migration/legacy_pairing.dart';
import '../core/storage/pairing_store.dart';
import 'home_shell.dart';
import 'legacy_offer_screen.dart';
import 'onboarding/onboarding_gate.dart';
import 'pairing_screen.dart';
import 'theme.dart';

class HerdrNotifyApp extends StatefulWidget {
  const HerdrNotifyApp({super.key, required this.store, this.migration});

  final PairingStore store;

  /// Null disables the offer of the Kotlin v1 pairing.
  final LegacyMigration? migration;

  @override
  State<HerdrNotifyApp> createState() => _HerdrNotifyAppState();
}

class _HerdrNotifyAppState extends State<HerdrNotifyApp> {
  Pairing? _pairing;
  LegacyPairing? _offer;
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    _boot();
  }

  Future<void> _boot() async {
    final pairing = await widget.store.load();
    // Only a phone with no pairing of its own is offered the old one.
    final offer = pairing.isComplete ? null : await widget.migration?.offer();
    if (!mounted) return;
    setState(() {
      _pairing = pairing;
      _offer = offer;
      _loading = false;
    });
  }

  void _onPaired(Pairing pairing) => setState(() {
    _pairing = pairing;
    _offer = null;
  });

  void _onNewPairing() => setState(() => _offer = null);

  Future<void> _onUnpaired() async {
    await widget.store.clear();
    if (!mounted) return;
    setState(() => _pairing = Pairing.empty);
  }

  @override
  Widget build(BuildContext context) {
    final pairing = _pairing;
    final offer = _offer;
    final migration = widget.migration;
    return MaterialApp(
      title: appName,
      debugShowCheckedModeBanner: false,
      theme: buildTheme(Brightness.light),
      darkTheme: buildTheme(),
      themeMode: ThemeMode.system,
      home: _loading || pairing == null
          ? Builder(builder: (context) => ColoredBox(color: context.colors.bg))
          : pairing.isComplete
          ? OnboardingGate(
              key: ValueKey(pairing.deviceId),
              pairing: pairing,
              home: HomeShell(pairing: pairing, onUnpaired: _onUnpaired),
            )
          : offer != null && migration != null
          ? LegacyOfferScreen(
              legacy: offer,
              migration: migration,
              onPaired: _onPaired,
              onNewPairing: _onNewPairing,
            )
          : PairingScreen(store: widget.store, onPaired: _onPaired),
    );
  }
}
