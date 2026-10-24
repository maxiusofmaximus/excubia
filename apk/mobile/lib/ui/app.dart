import 'package:flutter/material.dart';

import '../core/storage/pairing_store.dart';
import 'home_shell.dart';
import 'pairing_screen.dart';
import 'theme.dart';

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
      title: 'herdr-notify',
      debugShowCheckedModeBanner: false,
      theme: buildTheme(Brightness.light),
      darkTheme: buildTheme(),
      themeMode: ThemeMode.system,
      home: _loading || pairing == null
          ? Builder(builder: (context) => ColoredBox(color: context.colors.bg))
          : pairing.isComplete
          ? HomeShell(pairing: pairing, onUnpaired: _onUnpaired)
          : PairingScreen(store: widget.store, onPaired: _onPaired),
    );
  }
}
