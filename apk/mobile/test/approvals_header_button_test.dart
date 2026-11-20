import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/ui/chat/approval_entry.dart';
import 'package:excubia/ui/theme.dart';

Future<void> pumpShield(WidgetTester tester, int pending) async {
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      home: Scaffold(
        body: ApprovalsHeaderButton(pending: pending, onPressed: () {}),
      ),
    ),
  );
}

void main() {
  testWidgets('the shield is hidden when no approval waits', (tester) async {
    await pumpShield(tester, 0);

    expect(find.byIcon(Icons.shield_outlined), findsNothing);
    expect(find.byType(IconButton), findsNothing);
  });

  testWidgets('the shield shows with its count when approvals wait', (
    tester,
  ) async {
    await pumpShield(tester, 2);

    expect(find.byIcon(Icons.shield_outlined), findsOneWidget);
    expect(find.text('2'), findsOneWidget);
  });
}
