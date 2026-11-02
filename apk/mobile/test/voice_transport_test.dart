import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/api/pinned_client.dart';
import 'package:herdr_notify/src/rust/api/signing.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';

import 'support/fake_pinned_client.dart';
import 'support/pairing_fixtures.dart';

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  HostClient clientOn(FakePinnedClient fake) => HostClient(
    baseUrl: 'https://127.0.0.1:8443',
    deviceId: 'hn-7',
    privateKeyBase64: generateDeviceKey().privateKeyB64,
    pins: HostPins(primary: testPin),
    pinned: fake,
  );

  test('postBytes firma el cuerpo y lo envía con su Content-Type', () async {
    final fake = FakePinnedClient(
      answer: const PinnedAnswer(
        status: 200,
        body: '{"text":"x","ms":1}',
        address: '127.0.0.1:8443',
      ),
    );

    final answer = await clientOn(fake)
        .postBytes('/v1/voice/transcribe', [1, 2, 3], 'audio/ogg');

    expect(answer, '{"text":"x","ms":1}');
    expect(fake.targets, ['/v1/voice/transcribe']);
    expect(fake.bodies.single, [1, 2, 3]);
    expect(fake.headers.single[HttpHeaders.contentTypeHeader], 'audio/ogg');
    expect(fake.headers.single.keys, contains('X-HN-Signature'));
  });

  test('una llamada normal no añade Content-Type', () async {
    final fake = FakePinnedClient(
      answer: const PinnedAnswer(
        status: 200,
        body: '{"ok":true}',
        address: '127.0.0.1:8443',
      ),
    );

    await clientOn(fake).state();

    expect(
      fake.headers.single.containsKey(HttpHeaders.contentTypeHeader),
      isFalse,
    );
  });
}
