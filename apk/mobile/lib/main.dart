import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:excubia/src/rust/frb_generated.dart';

import 'core/alerts/alert_entry.dart';
import 'core/state/chat_document_opener.dart';
import 'core/storage/pairing_store.dart';
import 'ui/app.dart';
import 'ui/palette/palette_prefs.dart';

Future<void> main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  // The alerts service's engine: keeps the stream to the PC open, with no UI.
  if (args.contains(alertsBackgroundArg)) {
    await runAlertStream();
    return;
  }
  unawaited(pruneChatDocuments());
  SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
  SystemChrome.setSystemUIOverlayStyle(
    const SystemUiOverlayStyle(
      statusBarColor: Colors.transparent,
      systemNavigationBarColor: Colors.transparent,
    ),
  );
  final values = SecureKeyValueStore();
  final store = PairingStore(values);
  runApp(ExcubiaApp(store: store, palettePrefs: PalettePrefs(values)));
}
