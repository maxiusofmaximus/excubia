import 'dart:math';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/core/api/tailnet_cleartext.dart';
import 'package:herdr_notify/core/state/live_triage.dart';
import 'package:herdr_notify/core/storage/pairing_store.dart';

import 'support/memory_store.dart';

void main() {
  group('PairingStore', () {
    test('normalises the URL and trims the device id on save', () async {
      final store = PairingStore(MemoryKeyValueStore());
      await store.save(
        const Pairing(
          baseUrl: ' pc.tailnet.ts.net:8788/ ',
          deviceId: ' dev-abc ',
          privateKey: 'clave',
        ),
      );
      final loaded = await store.load();
      expect(loaded.baseUrl, 'https://pc.tailnet.ts.net:8788');
      expect(loaded.deviceId, 'dev-abc');
      expect(loaded.privateKey, 'clave');
      expect(loaded.isComplete, isTrue);
    });

    test('clear removes every fact, including the recovery code', () async {
      final memory = MemoryKeyValueStore();
      final store = PairingStore(memory);
      await store.save(
        const Pairing(baseUrl: 'https://x', deviceId: 'd', privateKey: 'k'),
      );
      await store.saveRecoveryCode('7QK3-M2XP');
      expect(memory.values[PairingStore.recoveryCodeKey], '7QK3-M2XP');
      await store.clear();
      final loaded = await store.load();
      expect(loaded.isComplete, isFalse);
      expect(memory.values, isEmpty);
    });

    test('the suggested device id is a host-side label', () {
      expect(
        RegExp(r'^hn-[0-9a-f]{8}$').hasMatch(PairingStore.suggestDeviceId()),
        isTrue,
      );
    });
  });

  group('backoff', () {
    LiveTriage triage(int seed) => LiveTriage(
      GatewayClient(
        baseUrl: 'http://100.64.0.1:8788',
        deviceId: 'dev-x',
        privateKeyBase64: 'placeholder',
      ),
      random: Random(seed),
    );

    test('grows exponentially, capped at 30 s, never below 1 s', () {
      final subject = triage(7);
      for (var attempt = 1; attempt <= 20; attempt++) {
        final wait = subject.backoffMillis(attempt);
        expect(wait, inInclusiveRange(1000, 30000));
      }
      expect(subject.backoffMillis(1), inInclusiveRange(800, 1200));
      expect(subject.backoffMillis(3), inInclusiveRange(3200, 4800));
      expect(subject.backoffMillis(20), inInclusiveRange(24000, 30000));
    });

    test('jitters, so two phones do not reconnect in the same instant', () {
      final subject = triage(3);
      final draws = List.generate(20, (_) => subject.backoffMillis(4));
      expect(draws.toSet().length, greaterThan(1));
    });
  });

  group('TailnetCleartext', () {
    test('cleartext is allowed only for tailnet-ish hosts', () {
      expect(
        TailnetCleartext.isCleartextAllowed(Uri.parse('https://any.example')),
        isTrue,
      );
      expect(
        TailnetCleartext.isCleartextAllowed(
          Uri.parse('http://100.64.0.1:8788'),
        ),
        isTrue,
      );
      expect(
        TailnetCleartext.isCleartextAllowed(
          Uri.parse('http://pc.tailnet.ts.net'),
        ),
        isTrue,
      );
      expect(
        TailnetCleartext.isCleartextAllowed(Uri.parse('http://127.0.0.1:8788')),
        isTrue,
      );
      expect(
        TailnetCleartext.isCleartextAllowed(Uri.parse('http://192.168.1.10')),
        isFalse,
      );
      expect(
        TailnetCleartext.isCleartextAllowed(Uri.parse('http://example.com')),
        isFalse,
      );
    });
  });
}
