import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/ui/theme.dart';
import 'package:excubia/ui/widgets/pairing_sas_dialog.dart';

void main() {
  Widget host() => MaterialApp(
    theme: buildTheme(),
    home: Builder(
      builder: (context) => Scaffold(
        body: TextButton(
          onPressed: () => showPairingSasDialog(context, '097665'),
          child: const Text('Abrir'),
        ),
      ),
    ),
  );

  testWidgets('shows the string in groups of three', (tester) async {
    await tester.pumpWidget(host());
    await tester.tap(find.text('Abrir'));
    await tester.pumpAndSettle();

    expect(find.text('Comprueba esta cadena'), findsOneWidget);
    expect(find.text('097 665'), findsOneWidget);
  });

  testWidgets('tapping outside does not close it', (tester) async {
    await tester.pumpWidget(host());
    await tester.tap(find.text('Abrir'));
    await tester.pumpAndSettle();

    await tester.tapAt(const Offset(5, 5));
    await tester.pumpAndSettle();

    expect(find.text('097 665'), findsOneWidget);
  });

  testWidgets('Entendido closes it', (tester) async {
    await tester.pumpWidget(host());
    await tester.tap(find.text('Abrir'));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Entendido'));
    await tester.pumpAndSettle();

    expect(find.text('Comprueba esta cadena'), findsNothing);
  });
}
