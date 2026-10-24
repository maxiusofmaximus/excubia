import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/pairing_address.dart';
import 'package:herdr_notify/core/api/pairing_api.dart';

void main() {
  group('PairRequest JSON', () {
    test('carries curve and public key first', () {
      expect(
        const PairRequest(publicKey: 'abc').toJson(),
        '{"curve":"p256","public_key":"abc"}',
      );
    });

    test('adds label and code only when they are not blank', () {
      expect(
        const PairRequest(
          publicKey: 'abc',
          label: 'Pixel 9',
          code: 'HJKQ',
        ).toJson(),
        '{"curve":"p256","public_key":"abc","label":"Pixel 9","code":"HJKQ"}',
      );
      expect(
        const PairRequest(publicKey: 'abc', label: '  ', code: '').toJson(),
        '{"curve":"p256","public_key":"abc"}',
      );
    });

    test('escapes quotes, backslashes and control characters', () {
      expect(
        const PairRequest(publicKey: 'a"b\\c\nd').toJson(),
        '{"curve":"p256","public_key":"a\\"b\\\\c\\nd"}',
      );
    });
  });

  group('address rules', () {
    test('a bare host is guessed https://', () {
      expect(
        PairingAddress.normaliseUrl('pc.tailnet.ts.net:8788/'),
        'https://pc.tailnet.ts.net:8788',
      );
    });

    test('an explicit scheme is kept and blank stays blank', () {
      expect(
        PairingAddress.normaliseUrl('http://100.64.0.1:8788'),
        'http://100.64.0.1:8788',
      );
      expect(PairingAddress.normaliseUrl('   '), '');
    });

    test('loopback is accepted only when the build policy allows it', () {
      expect(
        PairingAddress.loopbackProblem(
          'http://127.0.0.1:18878',
          allowLoopback: true,
        ),
        isNull,
      );
      expect(
        PairingAddress.loopbackProblem(
          'http://localhost:18878',
          allowLoopback: true,
        ),
        isNull,
      );
      expect(
        PairingAddress.loopbackProblem(
          'http://127.0.0.1:18878',
          allowLoopback: false,
        ),
        contains('depuración'),
      );
      expect(
        PairingAddress.loopbackProblem(
          'http://100.64.0.1:8788',
          allowLoopback: false,
        ),
        isNull,
      );
    });
  });
}
