import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';

import 'core/storage/pairing_store.dart';
import 'ui/app.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
  SystemChrome.setSystemUIOverlayStyle(
    const SystemUiOverlayStyle(
      statusBarColor: Colors.transparent,
      systemNavigationBarColor: Colors.transparent,
    ),
  );
  runApp(HerdrNotifyApp(store: PairingStore(SecureKeyValueStore())));
}
