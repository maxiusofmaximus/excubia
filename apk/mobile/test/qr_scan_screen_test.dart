import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/ui/qr_scan_screen.dart';
import 'package:herdr_notify/ui/theme.dart';

import 'support/fake_secure_screen.dart';

void main() {
  testWidgets('the scanner is secure while open and normal again after', (
    tester,
  ) async {
    final secure = FakeSecureScreen();
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Builder(
          builder: (context) => TextButton(
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute<void>(
                builder: (_) => QrScanScreen(secureScreen: secure),
              ),
            ),
            child: const Text('abrir'),
          ),
        ),
      ),
    );
    expect(secure.calls, isEmpty);

    await tester.tap(find.text('abrir'));
    await tester.pumpAndSettle();
    expect(secure.calls, [true]);

    await tester.tap(find.byIcon(Icons.close));
    await tester.pumpAndSettle();
    expect(secure.calls, [true, false]);
  });
}
