import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/pair_discovery.dart';

import 'support/fake_pair_discovery.dart';

const _channel = MethodChannel('dev.excubia/discovery');

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  final calls = <MethodCall>[];

  void answer(Future<Object?> Function(MethodCall call) handler) {
    calls.clear();
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(_channel, (call) {
          calls.add(call);
          return handler(call);
        });
  }

  tearDown(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(_channel, null);
  });

  group('MethodChannelPairDiscovery', () {
    test('an empty network is an empty list, not an error', () async {
      answer((_) async => <Object?>[]);

      expect(await const MethodChannelPairDiscovery().discover(), isEmpty);
    });

    test('hands back every address the phone found, in order', () async {
      answer((_) async => <Object?>['192.168.1.20:7443', '10.0.0.5:7443']);

      expect(await const MethodChannelPairDiscovery().discover(), [
        '192.168.1.20:7443',
        '10.0.0.5:7443',
      ]);
    });

    test('a platform error becomes the typed Spanish exception', () async {
      answer((_) async => throw PlatformException(code: 'discovery_failed'));

      await expectLater(
        const MethodChannelPairDiscovery().discover(),
        throwsA(
          isA<PairDiscoveryException>().having(
            (e) => e.message,
            'message',
            'No pude buscar el PC en la red. Comprueba la Wi-Fi.',
          ),
        ),
      );
    });

    test('waits 15 s by default and sends the timeout in milliseconds', () async {
      answer((_) async => <Object?>[]);

      await const MethodChannelPairDiscovery().discover();
      await const MethodChannelPairDiscovery().discover(
        timeout: const Duration(seconds: 3),
      );

      expect(calls.map((c) => c.method), ['discover', 'discover']);
      expect(calls[0].arguments, {'timeoutMs': 15000});
      expect(calls[1].arguments, {'timeoutMs': 3000});
    });
  });

  test('the fake records the timeout and can fail like the platform', () async {
    final fake = FakePairDiscovery(found: ['1.2.3.4:5']);

    expect(await fake.discover(), ['1.2.3.4:5']);
    expect(fake.timeouts, [const Duration(seconds: 15)]);

    fake.fails = true;
    await expectLater(fake.discover(), throwsA(isA<PairDiscoveryException>()));
  });
}
