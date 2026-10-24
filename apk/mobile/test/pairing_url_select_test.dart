import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/ui/theme.dart';
import 'package:herdr_notify/ui/widgets/advanced_pairing.dart';

void main() {
  testWidgets('tapping the stored gateway URL selects all of it', (
    tester,
  ) async {
    final url = TextEditingController(text: 'pc.old.ts.net:8788');
    final device = TextEditingController(text: 'telefono');
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Scaffold(
          body: SingleChildScrollView(
            child: AdvancedPairingSection(
              fingerprint: 'AB:CD',
              publicKeyCommand: 'host devices add',
              coreVersion: '0.1.2',
              deviceIdController: device,
              urlController: url,
              busy: false,
              onDeviceIdChanged: () {},
              onSaveAndContinue: () {},
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('Avanzado'));
    await tester.pumpAndSettle();
    await tester.tap(
      find.byWidgetPredicate((w) => w is TextField && w.controller == url),
    );
    await tester.pump();
    // Typing now replaces the stored value; a cursor at the end would append.
    expect(
      url.selection,
      TextSelection(baseOffset: 0, extentOffset: url.text.length),
    );
  });
}
