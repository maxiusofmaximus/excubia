import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/ui/theme.dart';
import 'package:excubia/ui/widgets/agent_portrait_card.dart';

Widget _host(Widget child) => MaterialApp(
  theme: buildTheme(),
  home: Scaffold(
    body: Center(child: SizedBox(width: 240, child: child)),
  ),
);

void main() {
  testWidgets('shows the name large and the tagline under it', (tester) async {
    await tester.pumpWidget(
      _host(
        const AgentPortraitCard(
          id: 'bob',
          name: 'Bob',
          tagline: 'Escribe programas',
        ),
      ),
    );
    expect(find.text('Bob'), findsOneWidget);
    expect(find.text('Escribe programas'), findsOneWidget);
    final name = tester.widget<Text>(find.text('Bob'));
    expect(name.style?.fontSize, 24);
  });

  testWidgets('its label is the name, with the tagline after it', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    await tester.pumpWidget(
      _host(
        const AgentPortraitCard(
          id: 'alfred',
          name: 'Alfred',
          tagline: 'Yo me ocupo',
        ),
      ),
    );
    expect(find.bySemanticsLabel('Alfred. Yo me ocupo'), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('without a tagline the label is only the name', (tester) async {
    final semantics = tester.ensureSemantics();
    await tester.pumpWidget(
      _host(const AgentPortraitCard(id: 'bob', name: 'Bob')),
    );
    expect(find.bySemanticsLabel('Bob'), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('shows its portrait, the neutral 600x750 render', (tester) async {
    await tester.pumpWidget(
      _host(const AgentPortraitCard(id: 'bob', name: 'Bob')),
    );
    final image = tester.widget<Image>(find.byType(Image));
    expect(
      (image.image as AssetImage).assetName,
      'assets/agents_v3/portraits/bob-neutral-600x750.webp',
    );
  });

  testWidgets('an agent without a render shows its initial, no picture', (
    tester,
  ) async {
    await tester.pumpWidget(
      _host(const AgentPortraitCard(id: '', name: 'Zoe')),
    );
    expect(find.text('Z'), findsOneWidget);
    expect(find.byType(Image), findsNothing);
  });

  testWidgets('a tap calls onTap and the card is a button', (tester) async {
    final semantics = tester.ensureSemantics();
    var taps = 0;
    await tester.pumpWidget(
      _host(AgentPortraitCard(id: 'bob', name: 'Bob', onTap: () => taps++)),
    );
    expect(
      tester.getSemantics(find.bySemanticsLabel('Bob')),
      matchesSemantics(isButton: true, hasTapAction: true, label: 'Bob'),
    );
    await tester.tap(find.text('Bob'));
    expect(taps, 1);
    semantics.dispose();
  });
}
