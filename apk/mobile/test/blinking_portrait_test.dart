import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/ui/theme.dart';
import 'package:excubia/ui/widgets/agent_portrait_card.dart';
import 'package:excubia/ui/widgets/blinking_portrait.dart';

const _base = 'assets/agents_v3/portraits/alfred-neutral-512.webp';
const _blink = 'assets/agents_v3/portraits/alfred-blink-512.webp';

/// Always the shortest gap, so the first blink comes at exactly 3 s.
class _ShortGaps implements Random {
  @override
  int nextInt(int max) => 0;
  @override
  bool nextBool() => false;
  @override
  double nextDouble() => 0;
}

Widget _host(
  Widget child, {
  bool reduceMotion = false,
  bool foreground = true,
}) => MaterialApp(
  theme: buildTheme(),
  home: MediaQuery(
    data: MediaQueryData(disableAnimations: reduceMotion),
    child: TickerMode(
      enabled: foreground,
      child: Scaffold(
        // Tall enough for a card: its 3:4 portrait and the name under it.
        body: Center(child: SizedBox(width: 240, height: 400, child: child)),
      ),
    ),
  ),
);

BlinkingPortrait _portrait({bool enabled = true}) => BlinkingPortrait(
  base: _base,
  blink: _blink,
  fallback: const SizedBox(),
  enabled: enabled,
  random: _ShortGaps(),
);

/// The blink overlay's opacity inside the widget: 1 while the eyes are shut,
/// 0 while open, null when there is no overlay at all.
double? _shut(WidgetTester tester) {
  final opacity = find.descendant(
    of: find.byType(BlinkingPortrait),
    matching: find.byType(Opacity),
  );
  return opacity.evaluate().isEmpty
      ? null
      : tester.widget<Opacity>(opacity).opacity;
}

void main() {
  testWidgets('blinks 3 s after mounting, and the base returns 120 ms later', (
    tester,
  ) async {
    await tester.pumpWidget(_host(_portrait()));

    await tester.pump(const Duration(milliseconds: 2999));
    expect(_shut(tester), 0.0);
    await tester.pump(const Duration(milliseconds: 1));
    expect(_shut(tester), 1.0);
    await tester.pump(const Duration(milliseconds: 119));
    expect(_shut(tester), 1.0);
    await tester.pump(const Duration(milliseconds: 1));
    expect(_shut(tester), 0.0);
  });

  testWidgets('with animations off it never blinks', (tester) async {
    await tester.pumpWidget(_host(_portrait(), reduceMotion: true));

    for (var i = 0; i < 400; i++) {
      await tester.pump(const Duration(milliseconds: 50));
      expect(_shut(tester), 0.0);
    }
  });

  testWidgets('with its route covered (no TickerMode) it never blinks', (
    tester,
  ) async {
    await tester.pumpWidget(_host(_portrait(), foreground: false));

    for (var i = 0; i < 400; i++) {
      await tester.pump(const Duration(milliseconds: 50));
      expect(_shut(tester), 0.0);
    }
  });

  testWidgets('disabled it has no overlay and never blinks', (tester) async {
    await tester.pumpWidget(_host(_portrait(enabled: false)));

    for (var i = 0; i < 400; i++) {
      await tester.pump(const Duration(milliseconds: 50));
      expect(_shut(tester), isNull);
    }
  });

  testWidgets('a character other than Alfred never blinks', (tester) async {
    await tester.pumpWidget(
      _host(const AgentPortraitCard(id: 'bob', name: 'Bob')),
    );

    for (var i = 0; i < 400; i++) {
      await tester.pump(const Duration(milliseconds: 50));
      expect(_shut(tester), isNull);
    }
  });

  testWidgets('the Alfred card carries the blink overlay of his render', (
    tester,
  ) async {
    await tester.pumpWidget(
      _host(const AgentPortraitCard(id: 'alfred', name: 'Alfred')),
    );

    expect(_shut(tester), isNotNull);
    final overlay = tester.widget<Image>(
      find.descendant(of: find.byType(Opacity), matching: find.byType(Image)),
    );
    expect(
      (overlay.image as AssetImage).assetName,
      'assets/agents_v3/portraits/alfred-blink-600x750.webp',
    );
  });

  testWidgets('unmounted it leaves no timer behind', (tester) async {
    await tester.pumpWidget(_host(_portrait()));
    await tester.pumpWidget(const SizedBox());

    // A timer left pending after the tree is gone fails the test on its own.
    await tester.pump(const Duration(seconds: 10));
  });
}
