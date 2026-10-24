import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/core/api/pairing_api.dart';
import 'package:herdr_notify/core/api/pinned_client.dart';
import 'package:herdr_notify/core/api/transport_note.dart';
import 'package:herdr_notify/core/storage/pairing_store.dart';
import 'package:herdr_notify/src/rust/api/pinned.dart' as rust;
import 'package:herdr_notify/src/rust/api/signing.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';

import 'support/fake_pinned_client.dart';
import 'support/memory_store.dart';
import 'support/pairing_fixtures.dart';

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  group('transport note', () {
    test('a pinned link is always named as verified TLS', () {
      expect(
        transportNote(baseUrl: '', pinned: true),
        'Conexión cifrada (TLS 1.3), verificada con la huella del código.',
      );
    });

    test('https is encrypted and says so', () {
      expect(
        transportNote(baseUrl: 'https://pc.tailnet.ts.net:8788', pinned: false),
        'Conexión cifrada (HTTPS).',
      );
    });

    test('plain http on loopback is unencrypted but stays on this machine', () {
      expect(
        transportNote(baseUrl: 'http://127.0.0.1:18878', pinned: false),
        'Sin cifrar, solo dentro de este equipo (loopback).',
      );
    });

    test('plain http off loopback is said plainly', () {
      expect(
        transportNote(baseUrl: 'http://192.168.1.20:8788', pinned: false),
        'Sin cifrar: el tráfico viaja en claro fuera de este equipo.',
      );
    });

    test('an empty address says nothing', () {
      expect(transportNote(baseUrl: '', pinned: false), isEmpty);
    });
  });

  group('pinned failure wording', () {
    test('a mismatch is the sentence the spec asks for', () {
      expect(
        pinnedFailureMessage(rust.PinnedFailureKind.pinMismatch),
        'La huella no coincide: no te conectes a esta red',
      );
    });

    test('an expired link is the sentence the spec asks for', () {
      expect(
        pinnedFailureMessage(rust.PinnedFailureKind.linkExpired),
        'El código ha caducado',
      );
    });
  });

  group('PairingApi pinned pairing', () {
    test('a matching host answers and its address is kept', () async {
      final fake = FakePinnedClient(
        answer: const PinnedAnswer(
          status: 200,
          body: '{"ok":true,"device_id":"hn-7","recovery_code":"ABCD-EFGH"}',
          address: '127.0.0.1:8443',
        ),
      );
      final response = await PairingApi(pinned: fake).pairPinned(
        addresses: const ['127.0.0.1:8443'],
        pins: HostPins(primary: testPin),
        request: const PairRequest(publicKey: 'pk', code: 'ABCD1234'),
      );

      expect(response.deviceId, 'hn-7');
      expect(response.recoveryCode, 'ABCD-EFGH');
      expect(response.address, '127.0.0.1:8443');
      expect(fake.targets, ['/v1/pair']);
      expect(fake.addresses, ['127.0.0.1:8443']);
    });

    test('a pin mismatch refuses with its kind and technical detail', () async {
      final fake = FakePinnedClient(
        failure: const PinnedException(
          rust.PinnedFailureKind.pinMismatch,
          'sha256/AAAA no coincide con las fijadas',
        ),
      );
      await expectLater(
        PairingApi(pinned: fake).pairPinned(
          addresses: const ['127.0.0.1:8443'],
          pins: HostPins(primary: testPin),
          request: const PairRequest(publicKey: 'pk', code: 'ABCD1234'),
        ),
        throwsA(
          isA<PairingException>()
              .having((e) => e.kind, 'kind', rust.PinnedFailureKind.pinMismatch)
              .having((e) => e.detail, 'detail', contains('no coincide')),
        ),
      );
    });

    test('a refused code keeps its HTTP status and has no kind', () async {
      final fake = FakePinnedClient(
        answer: const PinnedAnswer(
          status: 401,
          body: '',
          address: '127.0.0.1:8443',
        ),
      );
      await expectLater(
        PairingApi(pinned: fake).pairPinned(
          addresses: const ['127.0.0.1:8443'],
          pins: HostPins(primary: testPin),
          request: const PairRequest(publicKey: 'pk', code: 'NOPE'),
        ),
        throwsA(
          isA<PairingException>()
              .having((e) => e.status, 'status', 401)
              .having((e) => e.kind, 'kind', isNull),
        ),
      );
    });
  });

  group('GatewayClient with pins', () {
    test(
      'signed calls go through the pinned transport to the link address',
      () async {
        final fake = FakePinnedClient(
          answer: const PinnedAnswer(
            status: 200,
            body: '{"ok":true}',
            address: '127.0.0.1:8443',
          ),
        );
        final client = GatewayClient(
          baseUrl: 'https://127.0.0.1:8443',
          deviceId: 'hn-7',
          privateKeyBase64: generateDeviceKey().privateKeyB64,
          pins: HostPins(primary: testPin, backup: 'B' * 43),
          pinned: fake,
        );

        expect(await client.state(), '{"ok":true}');
        expect(fake.targets, ['/v1/state']);
        expect(fake.addresses, ['127.0.0.1:8443']);
        expect(fake.headers.single.keys, contains('X-HN-Signature'));
      },
    );
  });

  group('pairing store pins', () {
    test('pins survive a save and load, backup included', () async {
      final store = PairingStore(MemoryKeyValueStore());
      await store.save(
        Pairing(
          baseUrl: 'https://127.0.0.1:8443',
          deviceId: 'hn-7',
          privateKey: 'key',
          pins: HostPins(primary: testPin, backup: 'B' * 43),
        ),
      );

      final loaded = (await store.load()).pins;
      expect(loaded?.primary, testPin);
      expect(loaded?.backup, 'B' * 43);
    });

    test('a pairing without a backup loads with no backup', () async {
      final store = PairingStore(MemoryKeyValueStore());
      await store.save(
        Pairing(
          baseUrl: 'https://127.0.0.1:8443',
          deviceId: 'hn-7',
          privateKey: 'key',
          pins: HostPins(primary: testPin),
        ),
      );

      expect((await store.load()).pins?.backup, isNull);
    });

    test('an unpinned pairing drops pins kept from an earlier one', () async {
      final store = PairingStore(MemoryKeyValueStore());
      await store.save(
        Pairing(
          baseUrl: 'https://127.0.0.1:8443',
          deviceId: 'hn-7',
          privateKey: 'key',
          pins: HostPins(primary: testPin),
        ),
      );
      await store.save(
        const Pairing(
          baseUrl: 'http://100.64.0.1:8788',
          deviceId: 'hn-8',
          privateKey: 'key',
        ),
      );

      expect((await store.load()).pins, isNull);
    });

    test('clear removes the pins with the rest', () async {
      final store = PairingStore(MemoryKeyValueStore());
      await store.save(
        Pairing(
          baseUrl: 'https://127.0.0.1:8443',
          deviceId: 'hn-7',
          privateKey: 'key',
          pins: HostPins(primary: testPin),
        ),
      );
      await store.clear();

      expect((await store.load()).pins, isNull);
    });
  });
}
