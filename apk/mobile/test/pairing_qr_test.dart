import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/pair_qr.dart';

/// The QR parser is the only place an untrusted string becomes a pairing, so
/// every way the contract `herdr-notify://pair?u=<url>&c=<code>&v=1` can be
/// wrong — or hostile — is pinned here.
PairingQrPayload valid(String raw) {
  final result = PairingQrPayload.parse(raw);
  expect(result, isA<PairingQrValid>(), reason: 'esperaba válido: $result');
  return (result as PairingQrValid).payload;
}

String reason(String raw) {
  final result = PairingQrPayload.parse(raw);
  expect(result, isA<PairingQrInvalid>(), reason: 'esperaba inválido: $result');
  return (result as PairingQrInvalid).reason;
}

void main() {
  test('a well formed payload yields the url and the code', () {
    final payload = valid(
      'herdr-notify://pair?u=http%3A%2F%2Fpc.tailnet.ts.net%3A8788&c=ABCD1234&v=1',
    );
    expect(payload.baseUrl, 'http://pc.tailnet.ts.net:8788');
    expect(payload.code, 'ABCD1234');
  });

  test('a tailnet ip is accepted too', () {
    final payload = valid(
      'herdr-notify://pair?u=http%3A%2F%2F100.64.0.1%3A8788&c=7QK3M2XP&v=1',
    );
    expect(payload.baseUrl, 'http://100.64.0.1:8788');
  });

  test('an https address is accepted', () {
    final payload = valid(
      'herdr-notify://pair?u=https%3A%2F%2Fpc.tailnet.ts.net&c=CODE&v=1',
    );
    expect(payload.baseUrl, 'https://pc.tailnet.ts.net');
  });

  test('a loopback address is accepted for the adb-reverse host', () {
    final payload = valid(
      'herdr-notify://pair?u=http%3A%2F%2F127.0.0.1%3A18878&c=CODE&v=1',
    );
    expect(payload.baseUrl, 'http://127.0.0.1:18878');

    final localhost = valid(
      'herdr-notify://pair?u=http%3A%2F%2Flocalhost%3A18878&c=CODE&v=1',
    );
    expect(localhost.baseUrl, 'http://localhost:18878');
  });

  test('the schema version is required to be the one we know', () {
    expect(
      reason('herdr-notify://pair?u=http%3A%2F%2F100.64.0.1%3A8788&c=C&v=2'),
      contains('versión'),
    );
    expect(
      valid('herdr-notify://pair?u=http%3A%2F%2F100.64.0.1%3A8788&c=CODE&v=1')
          .code,
      'CODE',
    );
  });

  test('a foreign scheme is refused', () {
    expect(
      reason('https://pair?u=http%3A%2F%2F100.64.0.1%3A8788&c=C&v=1'),
      contains('herdr notify'),
    );
  });

  test('a foreign host is refused', () {
    expect(
      reason('herdr-notify://login?u=http%3A%2F%2F100.64.0.1%3A8788&c=C&v=1'),
      contains('emparejamiento'),
    );
  });

  test('a missing url is refused', () {
    expect(reason('herdr-notify://pair?c=ABCD&v=1'), contains('dirección'));
  });

  test('a missing code is refused', () {
    expect(
      reason('herdr-notify://pair?u=http%3A%2F%2F100.64.0.1%3A8788&v=1'),
      contains('código'),
    );
  });

  test('a url without a scheme is refused rather than guessed at', () {
    expect(
      reason('herdr-notify://pair?u=pc.tailnet.ts.net%3A8788&c=C&v=1'),
      contains('http'),
    );
  });

  test('surrounding spaces and newlines from a scan are trimmed', () {
    final payload = valid(
      '\n  herdr-notify://pair?u=http%3A%2F%2F100.64.0.1%3A8788&c=CODE&v=1 \n',
    );
    expect(payload.baseUrl, 'http://100.64.0.1:8788');
    expect(payload.code, 'CODE');
  });

  test('leading and trailing spaces inside a value do not change it', () {
    final payload = valid(
      'herdr-notify://pair?u=%20http%3A%2F%2F100.64.0.1%3A8788%20&c=%20CODE%20&v=1',
    );
    expect(payload.baseUrl, 'http://100.64.0.1:8788');
    expect(payload.code, 'CODE');
  });

  test('an over long payload is refused before it is parsed', () {
    expect(
      reason(
        'herdr-notify://pair?u=http%3A%2F%2F100.64.0.1&c='
        '${List<String>.filled(5000, 'A').join()}',
      ),
      contains('demasiado grande'),
    );
  });

  test('a duplicate key keeps the first value', () {
    final payload = valid(
      'herdr-notify://pair?u=http%3A%2F%2F100.64.0.1%3A8788&c=REAL&c=FAKE&v=1',
    );
    expect(payload.code, 'REAL');
  });

  test('extra parameters are ignored, not trusted', () {
    final payload = valid(
      'herdr-notify://pair?u=http%3A%2F%2F100.64.0.1%3A8788&c=CODE&v=1&evil=1',
    );
    expect(payload.baseUrl, 'http://100.64.0.1:8788');
    expect(payload.code, 'CODE');
  });

  test('an empty or blank payload is refused', () {
    expect(reason(''), contains('vacío'));
    expect(reason('   \n '), contains('vacío'));
  });

  test('an unparseable string is refused instead of crashing', () {
    expect(reason('herdr-notify://pair?a=b c'), isNotEmpty);
  });
}
