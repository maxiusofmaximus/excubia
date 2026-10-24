import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/pairing_entry.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';

import 'support/pairing_fixtures.dart';

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  test('a valid pinned link is read by the Rust core', () {
    final entry = parsePairingEntry(pinnedLink(expiresAt: nowSeconds() + 600));

    expect(entry, isA<PairingEntryPinned>());
    final link = (entry as PairingEntryPinned).link;
    expect(link.code, 'ABCD1234');
    expect(link.primary, testPin);
    expect(link.backup, isNull);
    expect(link.addresses, ['127.0.0.1:8443']);
  });

  test('surrounding whitespace from a paste does not matter', () {
    final entry = parsePairingEntry(
      '  ${pinnedLink(expiresAt: nowSeconds() + 600)}\n',
    );
    expect(entry, isA<PairingEntryPinned>());
  });

  test('an expired link is refused with the caducado sentence', () {
    final entry = parsePairingEntry(pinnedLink(expiresAt: nowSeconds() - 60));

    expect(entry, isA<PairingEntryInvalid>());
    expect(
      (entry as PairingEntryInvalid).reason,
      'El código ha caducado. Genera uno nuevo en el equipo.',
    );
  });

  test('a newer link version asks for an app update', () {
    final entry = parsePairingEntry(
      'herdr-notify://pair?v=2&c=ABCD1234&h=$testPin'
      '&a=127.0.0.1:8443&e=${nowSeconds() + 600}',
    );

    expect(
      (entry as PairingEntryInvalid).reason,
      contains('versión más nueva'),
    );
  });

  test('a malformed pin is a link that cannot be read', () {
    final entry = parsePairingEntry(
      'herdr-notify://pair?v=1&c=ABCD1234&h=short'
      '&a=127.0.0.1:8443&e=${nowSeconds() + 600}',
    );

    expect(
      (entry as PairingEntryInvalid).reason,
      'El enlace de emparejamiento no se pudo leer.',
    );
  });

  test('the earlier unpinned QR is still read, as a plain entry', () {
    final entry = parsePairingEntry(
      'herdr-notify://pair?u=http%3A%2F%2F100.64.0.1%3A8788&c=ABCD1234&v=1',
    );

    expect(entry, isA<PairingEntryPlain>());
    expect((entry as PairingEntryPlain).payload.code, 'ABCD1234');
  });

  test('text that is no link at all is refused', () {
    expect(parsePairingEntry('hola'), isA<PairingEntryInvalid>());
  });
}
