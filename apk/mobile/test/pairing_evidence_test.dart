import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/pairing_api.dart';
import 'package:herdr_notify/core/api/pinned_client.dart';
import 'package:herdr_notify/core/storage/pairing_store.dart';
import 'package:herdr_notify/src/rust/api/pinned.dart' as rust;
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/pairing_screen.dart';
import 'package:herdr_notify/ui/theme.dart';

import 'support/fake_pinned_client.dart';
import 'support/memory_store.dart';
import 'support/pairing_fixtures.dart';

/// Writes the CORE11B pairing screenshots. It runs only when HN_EVIDENCE_DIR
/// names a folder; HN_EVIDENCE_FONT may name a .ttf so the text is legible
/// (the test binding otherwise draws with Ahem, which is blocks).
void main() {
  final evidenceDir = Platform.environment['HN_EVIDENCE_DIR'];
  final skip = evidenceDir == null;
  final shotKey = GlobalKey();

  setUpAll(() async {
    await RustLib.init();
  });

  Future<void> loadFont(WidgetTester tester) async {
    final fontPath = Platform.environment['HN_EVIDENCE_FONT'];
    if (fontPath == null) return;
    final bytes = File(fontPath).readAsBytesSync();
    final loader = FontLoader('Roboto')
      ..addFont(Future.value(ByteData.sublistView(bytes)));
    await tester.runAsync(loader.load);
  }

  Future<void> capture(WidgetTester tester, String name) async {
    final boundary = tester.renderObject<RenderRepaintBoundary>(
      find.byKey(shotKey),
    );
    final image = await tester.runAsync(() => boundary.toImage());
    final png = await tester.runAsync(
      () => image!.toByteData(format: ui.ImageByteFormat.png),
    );
    File('$evidenceDir/$name.png').writeAsBytesSync(png!.buffer.asUint8List());
  }

  Future<void> mountPhone(WidgetTester tester, PinnedClient pinned) async {
    tester.view.physicalSize = const Size(412, 915);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    await loadFont(tester);
    await tester.pumpWidget(
      RepaintBoundary(
        key: shotKey,
        child: MaterialApp(
          theme: buildTheme(),
          home: PairingScreen(
            store: PairingStore(MemoryKeyValueStore()),
            onPaired: (_) {},
            api: PairingApi(pinned: pinned),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  testWidgets(
    'evidence: the pairing screen after a valid pinned link',
    skip: skip,
    (tester) async {
      await mountPhone(
        tester,
        FakePinnedClient(
          answer: const PinnedAnswer(
            status: 200,
            body: '{"ok":true,"device_id":"hn-7"}',
            address: '127.0.0.1:8443',
          ),
        ),
      );
      await pasteLink(tester, pinnedLink(expiresAt: nowSeconds() + 600));
      await capture(tester, 'pairing-valid-link');
    },
  );

  testWidgets('evidence: the pairing screen after a pin mismatch', skip: skip, (
    tester,
  ) async {
    await mountPhone(
      tester,
      FakePinnedClient(
        failure: const PinnedException(
          rust.PinnedFailureKind.pinMismatch,
          'sha256/AAAA no coincide con las fijadas',
        ),
      ),
    );
    await pasteLink(tester, pinnedLink(expiresAt: nowSeconds() + 600));
    await capture(tester, 'pairing-pin-mismatch');
  });
}
