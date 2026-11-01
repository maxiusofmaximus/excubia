import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/src/rust/api/state.dart';
import 'package:herdr_notify/ui/theme.dart';
import 'package:herdr_notify/ui/widgets/agent_avatar.dart';

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

/// A bundle in which every animated head is missing, as in an older build.
class _NoClipBundle extends CachingAssetBundle {
  @override
  Future<ByteData> load(String key) {
    if (key.endsWith('.webp')) {
      return Future.error(StateError('no clip in this build: $key'));
    }
    return rootBundle.load(key);
  }
}

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

  test('every status maps to its clip', () {
    expect(agentLookFor(AgentStatus.idle), AgentLook.idle);
    expect(agentLookFor(AgentStatus.unknown), AgentLook.idle);
    expect(agentLookFor(AgentStatus.working), AgentLook.working);
    expect(agentLookFor(AgentStatus.done), AgentLook.done);
    expect(agentLookFor(AgentStatus.blocked), AgentLook.error);
  });

  testWidgets('each status plays its clip, at 64 px up to 64 dp', (
    tester,
  ) async {
    await tester.pumpWidget(
      subject(
        const AgentAvatar(name: 'Bob', status: AgentStatus.working, size: 40),
      ),
    );
    expect(
      assetOf(tester).assetName,
      'assets/agents_anim/bob/working-head-64.webp',
    );

    await tester.pumpWidget(
      subject(
        const AgentAvatar(name: 'Bob', status: AgentStatus.blocked, size: 68),
      ),
    );
    expect(
      assetOf(tester).assetName,
      'assets/agents_anim/bob/error-head-128.webp',
    );

    await tester.pumpWidget(
      subject(
        const AgentAvatar(name: 'Bob', status: AgentStatus.done, size: 64),
      ),
    );
    expect(
      assetOf(tester).assetName,
      'assets/agents_anim/bob/done-head-64.webp',
    );
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
    expect(
      assetOf(tester).assetName,
      'assets/agents_anim/alfred/thinking-head-64.webp',
    );

    await tester.pumpWidget(
      subject(const AgentAvatar(name: 'Alfred', size: 68)),
    );
    expect(
      assetOf(tester).assetName,
      'assets/agents_anim/alfred/idle-head-128.webp',
    );
  });

  testWidgets(
    'the welcome glance plays look every 20 s, then returns to idle',
    (tester) async {
      await tester.pumpWidget(
        subject(const AgentAvatar(name: 'Alfred', size: 68, glance: true)),
      );
      expect(
        assetOf(tester).assetName,
        'assets/agents_anim/alfred/idle-head-128.webp',
      );

      await tester.pump(const Duration(seconds: 20));
      expect(
        assetOf(tester).assetName,
        'assets/agents_anim/alfred/look-head-128.webp',
      );

      await tester.pump(const Duration(seconds: 3));
      expect(
        assetOf(tester).assetName,
        'assets/agents_anim/alfred/idle-head-128.webp',
      );

      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets('reduce motion shows the static still', (tester) async {
    await tester.pumpWidget(
      subject(
        const AgentAvatar(name: 'Bob', status: AgentStatus.working, size: 68),
        reduceMotion: true,
      ),
    );
    expect(assetOf(tester).assetName, 'assets/agents/bob-working-128.png');
  });

  testWidgets('animated: false shows the static still', (tester) async {
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
    expect(assetOf(tester).assetName, 'assets/agents/bob-neutral-64.png');
  });

  testWidgets('a missing clip falls back to the still without an error', (
    tester,
  ) async {
    await tester.pumpWidget(
      DefaultAssetBundle(
        bundle: _NoClipBundle(),
        child: subject(
          const AgentAvatar(name: 'Bob', status: AgentStatus.done, size: 68),
        ),
      ),
    );
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 200)),
    );
    await tester.pump();

    // The failed WebP keeps its Image in the tree and paints the still inside
    // it, so both images are listed; the still is the one that is drawn.
    expect(tester.takeException(), isNull);
    final names = tester
        .widgetList<Image>(find.byType(Image))
        .map((image) => (image.image as AssetImage).assetName);
    expect(names, contains('assets/agents/bob-done-128.png'));
  });

  testWidgets('an agent without a render falls back to its initial', (
    tester,
  ) async {
    await tester.pumpWidget(
      subject(
        const AgentAvatar(name: 'Zoe', status: AgentStatus.blocked, size: 32),
      ),
    );

    expect(find.text('Z'), findsOneWidget);
    expect(find.byType(Image), findsNothing);
  });

  testWidgets(
    'Ernesto plays his clip and shows his still under reduce motion',
    (tester) async {
      await tester.pumpWidget(
        subject(
          const AgentAvatar(
            name: 'Ernesto',
            status: AgentStatus.working,
            size: 40,
          ),
        ),
      );
      expect(
        assetOf(tester).assetName,
        'assets/agents_anim/ernesto/working-head-64.webp',
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
      expect(
        assetOf(tester).assetName,
        'assets/agents/ernesto-working-128.png',
      );
    },
  );

  test('Ernesto\'s initial reaches 4.5:1 on his accent disc', () {
    final profile = agentProfileFor('Ernesto')!;
    expect(
      contrastRatio(profile.foreground, profile.background),
      greaterThanOrEqualTo(4.5),
    );
  });

  testWidgets('every clip of a rendered agent is bundled', (tester) async {
    await tester.runAsync(() async {
      for (final id in ['bob', 'ernesto']) {
        for (final look in AgentLook.values) {
          for (final size in ['64', '128']) {
            final key = 'assets/agents_anim/$id/${look.name}-head-$size.webp';
            final data = await rootBundle.load(key);
            expect(data.lengthInBytes, greaterThan(0), reason: key);
          }
        }
      }
    });
  });

  testWidgets('the animated head decodes to more than one frame', (
    tester,
  ) async {
    await tester.runAsync(() async {
      final data = await rootBundle.load(
        'assets/agents_anim/bob/working-head-128.webp',
      );
      final codec = await ui.instantiateImageCodec(data.buffer.asUint8List());
      expect(codec.frameCount, greaterThan(1));
      codec.dispose();
    });
  });
}
