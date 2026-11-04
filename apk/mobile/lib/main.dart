import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';

import 'core/alerts/alert_entry.dart';
import 'core/storage/pairing_store.dart';
import 'ui/app.dart';

Future<void> main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  // The alerts service's engine: keeps the stream to the PC open, with no UI.
  if (args.contains(alertsBackgroundArg)) {
    await runAlertStream();
    return;
  }
  SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
  SystemChrome.setSystemUIOverlayStyle(
    const SystemUiOverlayStyle(
      statusBarColor: Colors.transparent,
      systemNavigationBarColor: Colors.transparent,
    ),
  );
  final values = SecureKeyValueStore();
  final store = PairingStore(values);
  runApp(HerdrNotifyApp(store: store));
}
