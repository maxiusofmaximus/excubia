import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/src/rust/api/state.dart';
import 'package:excubia/ui/theme.dart';
import 'package:excubia/ui/widgets/agent_avatar.dart';
import 'package:excubia/ui/widgets/comic/comic_portrait.dart';

import 'support/contrast.dart';

Widget subject(Widget child, {bool reduceMotion = false}) => MaterialApp(
  theme: buildTheme(),
  home: Scaffold(
    body: Center(
      child: MediaQuery(
        data: MediaQueryData(disableAnimations: reduceMotion),
        child: child,
      ),
    ),
  ),
);

/// A bundle in which every portrait is missing, as in a build without them.
class _NoStillBundle extends CachingAssetBundle {
  @override
  Future<ByteData> load(String key) {
    if (key.startsWith('assets/portraits/')) {
      return Future.error(StateError('no still in this build: $key'));
    }
    return rootBundle.load(key);
  }
}

/// Every rendered agent; the ids the app ships a still for.
const _ids = [
  'alfred',
  'bob',
  'conan',
  'dani',
  'ernesto',
  'hugo',
  'ines',
  'nicole',
  'sami',
];

AssetImage assetOf(WidgetTester tester) =>
    tester.widget<Image>(find.byType(Image)).image as AssetImage;

void main() {
  test('a visible name finds its render, accents included', () {
    expect(agentProfileFor('Bob')?.id, 'bob');
    expect(agentProfileFor('Inés')?.id, 'ines');
    expect(agentProfileFor('Ernesto')?.id, 'ernesto');
    expect(agentProfileFor('Zoe'), isNull);
    expect(agentProfileFor(''), isNull);
  });

  test('the colours are the v3 manifest accents and deep tones', () {
    final bob = agentProfileById('bob')!;
    expect(bob.background, const Color(0xFF3CC9A8));
    expect(bob.foreground, const Color(0xFF0F7F6C));
    final ernesto = agentProfileById('ernesto')!;
    expect(ernesto.background, const Color(0xFFE09A5F));
    expect(ernesto.foreground, const Color(0xFF8A3E1F));
  });

  test('every status maps to its look', () {
    expect(agentLookFor(AgentStatus.idle), AgentLook.idle);
    expect(agentLookFor(AgentStatus.unknown), AgentLook.idle);
    expect(agentLookFor(AgentStatus.working), AgentLook.working);
    expect(agentLookFor(AgentStatus.done), AgentLook.done);
    expect(agentLookFor(AgentStatus.blocked), AgentLook.error);
  });

  testWidgets('each status shows its still: 128 px under 48 dp, 384 px above', (
    tester,
  ) async {
    await tester.pumpWidget(
      subject(
        const AgentAvatar(name: 'Bob', status: AgentStatus.working, size: 40),
      ),
    );
    await tester.pumpAndSettle();
    expect(assetOf(tester).assetName, 'assets/portraits/bob-working-128.webp');

    await tester.pumpWidget(
      subject(
        const AgentAvatar(name: 'Bob', status: AgentStatus.blocked, size: 68),
      ),
    );
    await tester.pumpAndSettle();
    expect(assetOf(tester).assetName, 'assets/portraits/bob-error-384.webp');

    await tester.pumpWidget(
      subject(
        const AgentAvatar(name: 'Bob', status: AgentStatus.done, size: 64),
      ),
    );
    await tester.pumpAndSettle();
    expect(assetOf(tester).assetName, 'assets/portraits/bob-done-384.webp');
  });

  testWidgets('thinking and look are forced over the status', (tester) async {
    await tester.pumpWidget(
      subject(
        const AgentAvatar(
          name: 'Alfred',
          status: AgentStatus.working,
          look: AgentLook.thinking,
          size: 32,
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(
      assetOf(tester).assetName,
      'assets/portraits/alfred-thinking-128.webp',
    );

    await tester.pumpWidget(
      subject(
        const AgentAvatar(name: 'Alfred', look: AgentLook.look, size: 68),
      ),
    );
    await tester.pumpAndSettle();
    expect(
      assetOf(tester).assetName,
      'assets/portraits/alfred-neutral-384.webp',
    );

    await tester.pumpWidget(
      subject(const AgentAvatar(name: 'Alfred', size: 68)),
    );
    await tester.pumpAndSettle();
    expect(
      assetOf(tester).assetName,
      'assets/portraits/alfred-neutral-384.webp',
    );
  });

  testWidgets('a status change fades to the new still in about 150 ms', (
    tester,
  ) async {
    await tester.pumpWidget(
      subject(
        const AgentAvatar(name: 'Bob', status: AgentStatus.idle, size: 68),
      ),
    );
    await tester.pumpAndSettle();
    await tester.pumpWidget(
      subject(
        const AgentAvatar(name: 'Bob', status: AgentStatus.working, size: 68),
      ),
    );
    // Halfway through the fade both stills are in the tree.
    await tester.pump(const Duration(milliseconds: 75));
    final names = tester
        .widgetList<Image>(find.byType(Image))
        .map((image) => (image.image as AssetImage).assetName);
    expect(names, contains('assets/portraits/bob-working-384.webp'));
    expect(names, contains('assets/portraits/bob-neutral-384.webp'));

    await tester.pump(const Duration(milliseconds: 150));
    expect(
      tester.widgetList<Image>(find.byType(Image)).length,
      1,
      reason: 'the old still leaves once the fade ends',
    );
  });

  testWidgets('reduce motion shows the new still at once, without a fade', (
    tester,
  ) async {
    await tester.pumpWidget(
      subject(
        const AgentAvatar(name: 'Bob', status: AgentStatus.idle, size: 68),
        reduceMotion: true,
      ),
    );
    await tester.pumpAndSettle();
    await tester.pumpWidget(
      subject(
        const AgentAvatar(name: 'Bob', status: AgentStatus.working, size: 68),
        reduceMotion: true,
      ),
    );
    await tester.pumpAndSettle();
    expect(
      tester.widgetList<Image>(find.byType(Image)).length,
      1,
      reason: 'no crossfade: only the new still is in the tree',
    );
    expect(assetOf(tester).assetName, 'assets/portraits/bob-working-384.webp');
  });

  testWidgets('animated: false shows the still without a fade', (tester) async {
    await tester.pumpWidget(
      subject(
        const AgentAvatar(
          name: 'Bob',
          status: AgentStatus.idle,
          size: 32,
          animated: false,
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(assetOf(tester).assetName, 'assets/portraits/bob-neutral-128.webp');
  });

  testWidgets('a missing still falls back to the initial without an error', (
    tester,
  ) async {
    await tester.pumpWidget(
      DefaultAssetBundle(
        bundle: _NoStillBundle(),
        child: subject(
          const AgentAvatar(name: 'Bob', status: AgentStatus.done, size: 68),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 200)),
    );
    await tester.pump();

    expect(tester.takeException(), isNull);
    expect(find.text('B'), findsOneWidget);
  });

  testWidgets('an agent without a render falls back to its initial', (
    tester,
  ) async {
    await tester.pumpWidget(
      subject(
        const AgentAvatar(name: 'Zoe', status: AgentStatus.blocked, size: 32),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Z'), findsOneWidget);
    expect(find.byType(Image), findsNothing);
  });

  testWidgets('Ernesto shows his still at both sizes', (tester) async {
    await tester.pumpWidget(
      subject(
        const AgentAvatar(
          name: 'Ernesto',
          status: AgentStatus.working,
          size: 40,
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(
      assetOf(tester).assetName,
      'assets/portraits/ernesto-working-128.webp',
    );

    await tester.pumpWidget(
      subject(
        const AgentAvatar(
          name: 'Ernesto',
          status: AgentStatus.working,
          size: 68,
        ),
        reduceMotion: true,
      ),
    );
    await tester.pumpAndSettle();
    expect(
      assetOf(tester).assetName,
      'assets/portraits/ernesto-working-384.webp',
    );
  });

  testWidgets('no portrait is round: no ClipOval and no CircleAvatar', (
    tester,
  ) async {
    await tester.pumpWidget(
      subject(
        const Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            AgentAvatar(name: 'Alfred', size: 64, framed: true),
            AgentAvatar(name: 'Bob', size: 40),
            AgentAvatar(name: 'Bob', size: 40, ringed: true),
          ],
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.byType(ClipOval), findsNothing);
    expect(find.byType(CircleAvatar), findsNothing);
    expect(find.byType(ComicPortrait), findsOneWidget);
  });

  test('every initial reaches 4.5:1 on its accent disc', () {
    for (final id in _ids) {
      final profile = agentProfileById(id)!;
      expect(
        contrastRatio(agentInitialInk, profile.background),
        greaterThanOrEqualTo(4.5),
        reason: profile.id,
      );
    }
  });

  testWidgets('every id, look and size asks for a bundled portrait', (
    tester,
  ) async {
    await tester.runAsync(() async {
      for (final id in _ids) {
        final profile = agentProfileById(id)!;
        for (final look in [
          AgentLook.idle,
          AgentLook.thinking,
          AgentLook.working,
          AgentLook.done,
          AgentLook.error,
        ]) {
          for (final size in [40.0, 64.0, 128.0, 220.0]) {
            final key = stillAssetFor(profile, look, size);
            final data = await rootBundle.load(key);
            expect(data.lengthInBytes, greaterThan(0), reason: key);
          }
        }
      }
    });
  });

  testWidgets('a portrait decodes to its size', (tester) async {
    await tester.runAsync(() async {
      final data = await rootBundle.load(
        'assets/portraits/bob-working-128.webp',
      );
      final codec = await ui.instantiateImageCodec(data.buffer.asUint8List());
      final frame = await codec.getNextFrame();
      expect(frame.image.width, 128);
      frame.image.dispose();
      codec.dispose();
    });
  });
}
