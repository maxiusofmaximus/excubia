import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/pair_discovery.dart';
import 'package:excubia/core/api/pairing_api.dart';
import 'package:excubia/core/api/pake_pairing.dart';
import 'package:excubia/core/storage/pairing_store.dart';
import 'package:excubia/src/rust/frb_generated.dart';
import 'package:excubia/ui/pairing_screen.dart';
import 'package:excubia/ui/theme.dart';

import 'support/fake_pair_discovery.dart';
import 'support/fake_pinned_client.dart';
import 'support/memory_store.dart';
import 'support/pairing_fixtures.dart';

const _code = 'KQ7M2XW9';
const _found = '192.168.1.20:47443';
const _none = <String>[];

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  group('discoverWithRetry', () {
    test('stops at the first window that finds something', () async {
      final discovery = FakePairDiscovery(found: const [_found]);

      expect(await discoverWithRetry(discovery), [_found]);
      expect(discovery.timeouts, [const Duration(seconds: 7)]);
    });

    test('tries again after an empty window', () async {
      final discovery = FakePairDiscovery(
        script: const [
          _none,
          _none,
          [_found],
        ],
      );

      expect(await discoverWithRetry(discovery), [_found]);
      expect(discovery.timeouts, hasLength(3));
    });

    test(
      'gives up empty after the attempts, with the time it is given',
      () async {
        final discovery = FakePairDiscovery();

        const short = Duration(seconds: 1);
        expect(
          await discoverWithRetry(discovery, attempts: 2, timeout: short),
          isEmpty,
        );
        expect(discovery.timeouts, [short, short]);
      },
    );

    test('a platform exception is not retried', () async {
      final discovery = FakePairDiscovery(fails: true);

      await expectLater(
        discoverWithRetry(discovery),
        throwsA(isA<PairDiscoveryException>()),
      );
      expect(discovery.timeouts, hasLength(1));
    });

    test('after an empty window the fallback sweep finds it', () async {
      final discovery = FakePairDiscovery();
      final sweep = FakePairDiscovery(found: const [_found]);

      expect(await discoverWithRetry(discovery, fallback: sweep), [_found]);
      expect(discovery.timeouts, hasLength(1));
      expect(sweep.timeouts, [const Duration(seconds: 4)]);
    });

    test('an empty sweep leaves the other windows to run', () async {
      final discovery = FakePairDiscovery(
        script: const [
          _none,
          _none,
          [_found],
        ],
      );
      final sweep = FakePairDiscovery();

      expect(await discoverWithRetry(discovery, fallback: sweep), [_found]);
      expect(discovery.timeouts, hasLength(3));
      expect(sweep.timeouts, hasLength(1));
    });

    test('a window with a find never asks the fallback', () async {
      final discovery = FakePairDiscovery(found: const [_found]);
      final sweep = FakePairDiscovery(found: const ['10.0.0.5:47443']);

      expect(await discoverWithRetry(discovery, fallback: sweep), [_found]);
      expect(sweep.timeouts, isEmpty);
    });
  });

  group('the pairing screen', () {
    late PairingStore store;

    setUp(() => store = PairingStore(MemoryKeyValueStore()));

    Future<FakePinnedClient> pairWith(
      WidgetTester tester,
      FakePairDiscovery discovery,
    ) async {
      final pinned = FakePinnedClient(
        pake: PakePaired(
          deviceId: 'hn-9',
          recoveryCode: 'RECO-VERY-0001',
          pin: testPin,
          address: _found,
        ),
      );
      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(),
          home: PairingScreen(
            store: store,
            onPaired: (_) {},
            api: PairingApi(pinned: pinned),
            discovery: discovery,
          ),
        ),
      );
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField).first, _code);
      await tester.ensureVisible(find.text('Emparejar'));
      await tester.tap(find.text('Emparejar'));
      await tester.pumpAndSettle();
      return pinned;
    }

    testWidgets('empty, empty, then found pairs with what it found', (
      tester,
    ) async {
      final discovery = FakePairDiscovery(
        script: const [
          _none,
          _none,
          [_found],
        ],
      );
      final pinned = await pairWith(tester, discovery);

      expect(discovery.timeouts, hasLength(3));
      expect(pinned.pakeCalls.single.addresses, [_found]);
    });

    testWidgets('found at once searches once', (tester) async {
      final discovery = FakePairDiscovery(found: const [_found]);
      final pinned = await pairWith(tester, discovery);

      expect(discovery.timeouts, hasLength(1));
      expect(pinned.pakeCalls, hasLength(1));
    });

    testWidgets('three empty windows say no PC and never pair', (tester) async {
      final discovery = FakePairDiscovery();
      final pinned = await pairWith(tester, discovery);

      expect(discovery.timeouts, hasLength(3));
      expect(find.textContaining('No encontré el PC'), findsOneWidget);
      expect(pinned.pakeCalls, isEmpty);
    });

    testWidgets('an exception in the first window ends the search', (
      tester,
    ) async {
      final discovery = FakePairDiscovery(fails: true);
      final pinned = await pairWith(tester, discovery);

      expect(discovery.timeouts, hasLength(1));
      expect(find.text(const PairDiscoveryException().message), findsOneWidget);
      expect(pinned.pakeCalls, isEmpty);
    });
  });
}
