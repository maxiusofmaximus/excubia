import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/pairing_api.dart';
import 'package:herdr_notify/core/migration/legacy_migration.dart';
import 'package:herdr_notify/core/migration/legacy_pairing.dart';
import 'package:herdr_notify/core/storage/pairing_store.dart';
import 'package:herdr_notify/src/rust/api/signing.dart';
import 'package:herdr_notify/ui/legacy_offer_screen.dart';

import 'support/memory_store.dart';

const MethodChannel _channel = MethodChannel('dev.herdrnotify/legacy_pairing');

/// Replaces the platform side of the Kotlin channel for one test.
void _mockChannel(Future<Object?> Function(MethodCall call) handler) {
  TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
      .setMockMethodCallHandler(_channel, handler);
}

class FakePairingApi extends PairingApi {
  FakePairingApi(this.reply);

  final Future<PairResponse> Function(String baseUrl, PairRequest request)
  reply;
  final List<PairRequest> requests = <PairRequest>[];

  @override
  Future<PairResponse> pair(String baseUrl, PairRequest request) {
    requests.add(request);
    return reply(baseUrl, request);
  }
}

class FakeLegacySource implements LegacyPairingSource {
  FakeLegacySource({this.pairing, this.code});

  LegacyPairing? pairing;
  String? code;
  bool cleared = false;

  @override
  Future<LegacyPairing?> read() async => pairing;

  @override
  Future<String?> recoveryCode() async => code;

  @override
  Future<void> clear() async => cleared = true;
}

class _Unreachable implements Exception {
  const _Unreachable();
}

const LegacyPairing _legacy = LegacyPairing(
  baseUrl: 'http://100.64.0.7',
  deviceId: 'hn-1a2b3c4d',
);

DeviceKeyMaterial _material() => const DeviceKeyMaterial(
  privateKeyB64: 'new-private',
  publicKeyB64: 'new-public',
  fingerprint: 'aa:bb',
);

PairResponse _ok({String recovery = 'NEWC-ODE1'}) =>
    PairResponse(deviceId: 'hn-1a2b3c4d', recoveryCode: recovery);

Future<PairResponse> _refuse(int status) =>
    Future<PairResponse>.error(PairingException(status));

LegacyMigration _migration({
  required FakeLegacySource source,
  required MemoryKeyValueStore values,
  PairingStore? store,
  FakePairingApi? api,
  Future<void> Function(Pairing candidate)? verify,
}) => LegacyMigration(
  source: source,
  values: values,
  store: store ?? PairingStore(values),
  api: api ?? FakePairingApi((_, _) => Future.value(_ok())),
  newKey: _material,
  verify: verify ?? (_) async {},
);

void main() {
  tearDown(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(_channel, null);
  });

  group('MethodChannelLegacyPairingSource.read', () {
    test('sin datos legados devuelve null', () async {
      _mockChannel((call) async => null);
      expect(await MethodChannelLegacyPairingSource().read(), isNull);
    });

    test('datos válidos: normaliza la URL y recorta el id', () async {
      _mockChannel(
        (call) async => {'baseUrl': '100.64.0.7', 'deviceId': ' hn-1a2b3c4d '},
      );
      final pairing = await MethodChannelLegacyPairingSource().read();
      expect(pairing?.baseUrl, 'http://100.64.0.7');
      expect(pairing?.deviceId, 'hn-1a2b3c4d');
      expect(pairing?.hostLabel, '100.64.0.7');
    });

    for (final corrupt in <Object?>[
      <String, Object?>{'baseUrl': '100.64.0.7', 'deviceId': ''},
      <String, Object?>{'baseUrl': '   ', 'deviceId': 'hn-1'},
      <String, Object?>{'baseUrl': 42, 'deviceId': 'hn-1'},
      <String, Object?>{'deviceId': 'hn-1'},
      'no es un mapa',
    ]) {
      test('datos corruptos ($corrupt) se ignoran sin error', () async {
        _mockChannel((call) async => corrupt);
        expect(await MethodChannelLegacyPairingSource().read(), isNull);
      });
    }

    test('un fallo de la plataforma se lee como ausencia de datos', () async {
      _mockChannel(
        (call) async =>
            throw PlatformException(code: 'legacy_pairing', message: 'x'),
      );
      expect(await MethodChannelLegacyPairingSource().read(), isNull);
    });
  });

  group('MethodChannelLegacyPairingSource.recoveryCode and clear', () {
    test('un código vacío equivale a no tener código', () async {
      _mockChannel((call) async => '  ');
      expect(await MethodChannelLegacyPairingSource().recoveryCode(), isNull);
    });

    test('devuelve el código sin espacios sobrantes', () async {
      _mockChannel((call) async => ' ABCD-1234 ');
      expect(
        await MethodChannelLegacyPairingSource().recoveryCode(),
        'ABCD-1234',
      );
    });

    test('clear llama al método y un fallo no se propaga', () async {
      final calls = <String>[];
      _mockChannel((call) async {
        calls.add(call.method);
        throw PlatformException(code: 'legacy_pairing');
      });
      await MethodChannelLegacyPairingSource().clear();
      expect(calls, ['clear']);
    });
  });

  group('LegacyMigration.offer', () {
    test('sin datos legados no hay oferta', () async {
      final migration = _migration(
        source: FakeLegacySource(),
        values: MemoryKeyValueStore(),
      );
      expect(await migration.offer(), isNull);
    });

    test('datos legados válidos se ofrecen', () async {
      final migration = _migration(
        source: FakeLegacySource(pairing: _legacy, code: 'OLD'),
        values: MemoryKeyValueStore(),
      );
      expect((await migration.offer())?.hostLabel, '100.64.0.7');
    });

    test('tras «Emparejar de nuevo» no vuelve a ofrecerse', () async {
      final values = MemoryKeyValueStore();
      final source = FakeLegacySource(pairing: _legacy, code: 'OLD');
      final first = _migration(source: source, values: values);
      expect(await first.offer(), isNotNull);

      await first.decline();

      final again = _migration(source: source, values: values);
      expect(await again.offer(), isNull);
    });
  });

  group('LegacyMigration.adopt', () {
    test('sin código de recuperación no llama al host', () async {
      final api = FakePairingApi((_, _) => Future.value(_ok()));
      final source = FakeLegacySource(pairing: _legacy);
      final migration = _migration(
        source: source,
        values: MemoryKeyValueStore(),
        api: api,
      );

      final result = await migration.adopt(_legacy);

      expect(result.outcome, LegacyOutcome.noRecoveryCode);
      expect(api.requests, isEmpty);
      expect(source.cleared, isFalse);
    });

    test('el host rechaza el código (403): se conserva lo legado', () async {
      final values = MemoryKeyValueStore();
      final store = PairingStore(values);
      final source = FakeLegacySource(pairing: _legacy, code: 'OLD');
      final migration = _migration(
        source: source,
        values: values,
        store: store,
        api: FakePairingApi((_, _) => _refuse(403)),
      );

      final result = await migration.adopt(_legacy);

      expect(result.outcome, LegacyOutcome.rejected);
      expect(result.pairing, isNull);
      expect((await store.load()).isComplete, isFalse);
      expect(source.cleared, isFalse);
    });

    test('equipo inalcanzable (status 0): se conserva lo legado', () async {
      final source = FakeLegacySource(pairing: _legacy, code: 'OLD');
      final migration = _migration(
        source: source,
        values: MemoryKeyValueStore(),
        api: FakePairingApi((_, _) => _refuse(0)),
      );

      expect(
        (await migration.adopt(_legacy)).outcome,
        LegacyOutcome.unreachable,
      );
      expect(source.cleared, isFalse);
    });

    test(
      'el re-enrolamiento pasa pero la comprobación firmada falla: '
      'nada se guarda y lo legado se conserva',
      () async {
        final values = MemoryKeyValueStore();
        final store = PairingStore(values);
        final source = FakeLegacySource(pairing: _legacy, code: 'OLD');
        final migration = _migration(
          source: source,
          values: values,
          store: store,
          api: FakePairingApi((_, _) => Future.value(_ok())),
          verify: (_) => Future<void>.error(const _Unreachable()),
        );

        final result = await migration.adopt(_legacy);

        expect(result.outcome, LegacyOutcome.unverified);
        expect((await store.load()).isComplete, isFalse);
        expect(await values.read(PairingStore.recoveryCodeKey), isNull);
        expect(source.cleared, isFalse);
      },
    );

    test('éxito: guarda el nuevo emparejamiento y borra lo legado', () async {
      final values = MemoryKeyValueStore();
      final store = PairingStore(values);
      final source = FakeLegacySource(pairing: _legacy, code: 'OLD');
      final api = FakePairingApi((_, _) => Future.value(_ok()));
      var storedDuringVerify = true;
      final migration = _migration(
        source: source,
        values: values,
        store: store,
        api: api,
        verify: (candidate) async {
          // The check runs before anything is written to this phone.
          storedDuringVerify = (await store.load()).isComplete;
          expect(candidate.privateKey, 'new-private');
        },
      );

      final result = await migration.adopt(_legacy);

      expect(result.outcome, LegacyOutcome.migrated);
      expect(storedDuringVerify, isFalse);
      expect(source.cleared, isTrue);

      final saved = await store.load();
      expect(saved.baseUrl, 'http://100.64.0.7');
      expect(saved.deviceId, 'hn-1a2b3c4d');
      expect(saved.privateKey, 'new-private');
      expect(await values.read(PairingStore.recoveryCodeKey), 'NEWC-ODE1');
      expect(result.pairing?.privateKey, 'new-private');

      expect(api.requests.single.code, 'OLD');
      expect(api.requests.single.publicKey, 'new-public');
      expect(api.requests.single.label, 'hn-1a2b3c4d');
    });
  });

  group('LegacyOfferScreen', () {
    Future<void> pumpOffer(
      WidgetTester tester, {
      required FakePairingApi api,
      required FakeLegacySource source,
      required MemoryKeyValueStore values,
      required void Function(Pairing) onPaired,
      required void Function() onNewPairing,
    }) => tester.pumpWidget(
      MaterialApp(
        home: LegacyOfferScreen(
          legacy: _legacy,
          migration: _migration(source: source, values: values, api: api),
          onPaired: onPaired,
          onNewPairing: onNewPairing,
        ),
      ),
    );

    testWidgets('pregunta por el equipo anterior, sin tecnicismos', (
      tester,
    ) async {
      await pumpOffer(
        tester,
        api: FakePairingApi((_, _) => Future.value(_ok())),
        source: FakeLegacySource(pairing: _legacy, code: 'OLD'),
        values: MemoryKeyValueStore(),
        onPaired: (_) {},
        onNewPairing: () {},
      );

      expect(
        find.text(
          'Encontré el emparejamiento de la versión anterior con 100.64.0.7. ¿Usarlo?',
        ),
        findsOneWidget,
      );
      expect(find.text('Usar'), findsOneWidget);
      expect(find.text('Emparejar de nuevo'), findsOneWidget);
    });

    testWidgets('«Usar» con el host que rechaza: explica y ofrece repetir', (
      tester,
    ) async {
      await pumpOffer(
        tester,
        api: FakePairingApi((_, _) => _refuse(403)),
        source: FakeLegacySource(pairing: _legacy, code: 'OLD'),
        values: MemoryKeyValueStore(),
        onPaired: (_) => fail('no debe emparejar'),
        onNewPairing: () {},
      );

      await tester.tap(find.text('Usar'));
      await tester.pumpAndSettle();

      expect(
        find.textContaining('ya no reconoce este teléfono'),
        findsOneWidget,
      );
      expect(find.text('Usar'), findsNothing);
      expect(find.text('Emparejar de nuevo'), findsOneWidget);
    });

    testWidgets('«Usar» con éxito entrega el emparejamiento', (tester) async {
      Pairing? paired;
      await pumpOffer(
        tester,
        api: FakePairingApi((_, _) => Future.value(_ok())),
        source: FakeLegacySource(pairing: _legacy, code: 'OLD'),
        values: MemoryKeyValueStore(),
        onPaired: (pairing) => paired = pairing,
        onNewPairing: () {},
      );

      await tester.tap(find.text('Usar'));
      await tester.pumpAndSettle();

      expect(paired?.deviceId, 'hn-1a2b3c4d');
      expect(paired?.privateKey, 'new-private');
    });

    testWidgets('«Emparejar de nuevo» no vuelve a ofrecer lo legado', (
      tester,
    ) async {
      final values = MemoryKeyValueStore();
      var newPairing = false;
      await pumpOffer(
        tester,
        api: FakePairingApi((_, _) => Future.value(_ok())),
        source: FakeLegacySource(pairing: _legacy, code: 'OLD'),
        values: values,
        onPaired: (_) {},
        onNewPairing: () => newPairing = true,
      );

      await tester.tap(find.text('Emparejar de nuevo'));
      await tester.pumpAndSettle();

      expect(newPairing, isTrue);
      expect(await values.read('hn_legacy_offer_declined'), '1');
    });
  });
}
