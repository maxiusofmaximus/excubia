import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// A 43-character base64url key: the shape of a real SPKI pin.
final String testPin = 'A' * 43;

/// Unix seconds, the unit of the link's `e=` field.
int nowSeconds() => DateTime.now().millisecondsSinceEpoch ~/ 1000;

/// A CORE11 pairing link that expires at [expiresAt] (unix seconds).
String pinnedLink({required int expiresAt, String code = 'ABCD1234'}) =>
    'herdr-notify://pair?v=1&c=$code&h=$testPin'
    '&a=127.0.0.1:8443&e=$expiresAt';

/// Types [link] into the "Pegar enlace" dialog and confirms it, the way the
/// operator does on a phone without a camera view of the QR.
Future<void> pasteLink(WidgetTester tester, String link) async {
  await tester.tap(find.text('Pegar enlace'));
  await tester.pumpAndSettle();
  await tester.enterText(
    find.descendant(
      of: find.byType(AlertDialog),
      matching: find.byType(TextField),
    ),
    link,
  );
  await tester.tap(find.text('Usar'));
  await tester.pumpAndSettle();
}
