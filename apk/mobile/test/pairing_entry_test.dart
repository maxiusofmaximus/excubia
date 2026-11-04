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

  test('a link printed before the rename is refused like any unknown link', () {
    final expires = nowSeconds() + 600;
    for (final link in [
      pinnedLink(expiresAt: expires).replaceFirst('excubia:', 'herdr-notify:'),
      pakeLink(expiresAt: expires).replaceFirst('excubia:', 'herdr-notify:'),
      'herdr-notify://pair?u=http%3A%2F%2F100.64.0.1%3A8788&c=ABCD1234&v=1',
    ]) {
      final entry = parsePairingEntry(link);
      expect(entry, isA<PairingEntryInvalid>(), reason: link);
      expect((entry as PairingEntryInvalid).reason, contains('no es de Excubia'));
    }
  });

  test('a scheme that only looks like ours is refused', () {
    final entry = parsePairingEntry(
      pinnedLink(
        expiresAt: nowSeconds() + 600,
      ).replaceFirst('excubia:', 'excubia-x:'),
    );

    expect(entry, isA<PairingEntryInvalid>());
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
      'excubia://pair?v=3&c=ABCD1234&h=$testPin'
      '&a=127.0.0.1:8443&e=${nowSeconds() + 600}',
    );

    expect(
      (entry as PairingEntryInvalid).reason,
      contains('versión más nueva'),
    );
  });

  test('a malformed pin is a link that cannot be read', () {
    final entry = parsePairingEntry(
      'excubia://pair?v=1&c=ABCD1234&h=short'
      '&a=127.0.0.1:8443&e=${nowSeconds() + 600}',
    );

    expect(
      (entry as PairingEntryInvalid).reason,
      'El enlace de emparejamiento no se pudo leer.',
    );
  });

  group('v2 QR', () {
    const invalid = 'El QR no es válido. Genera otro en el PC.';

    test('a valid one carries code, host pin and expiry', () {
      final expires = nowSeconds() + 120;
      final entry = parsePairingEntry(pakeLink(expiresAt: expires));

      expect(entry, isA<PairingEntryPake>());
      final pake = entry as PairingEntryPake;
      expect(pake.code, 'KQ7M2XW9');
      expect(pake.hostFingerprint, testPin);
      expect(pake.expiresAt.millisecondsSinceEpoch, expires * 1000);
      expect(pake.toString(), isNot(contains('KQ7M2XW9')));
    });

    test('an expired one is refused by the injected clock, not the real one', () {
      final expires = nowSeconds() + 120;
      PairingEntry at(int seconds) => parsePairingEntry(
        pakeLink(expiresAt: expires),
        clock: () => DateTime.fromMillisecondsSinceEpoch(seconds * 1000),
      );

      expect(at(expires - 1), isA<PairingEntryPake>());
      for (final late in [expires, expires + 60]) {
        expect(
          (at(late) as PairingEntryInvalid).reason,
          'El QR caducó. Genera otro en el PC.',
        );
      }
    });

    test('an address, a short code or a bad pin is refused without echo', () {
      final e = nowSeconds() + 120;
      for (final link in [
        '${pakeLink(expiresAt: e)}&a=127.0.0.1:8443',
        pakeLink(expiresAt: e, code: 'KQ7M2'),
        'excubia://pair?v=2&c=KQ7M2XW9&h=short&e=$e',
      ]) {
        final reason = (parsePairingEntry(link) as PairingEntryInvalid).reason;
        expect(reason, invalid);
        expect(reason, isNot(contains('KQ7M2')));
      }
    });

    test('a v3 link asks for an app update', () {
      final entry = parsePairingEntry(
        'excubia://pair?v=3&c=KQ7M2XW9&h=$testPin&e=${nowSeconds() + 120}',
      );
      expect(
        (entry as PairingEntryInvalid).reason,
        contains('versión más nueva'),
      );
    });
  });

  test('the retired unpinned u= QR is refused', () {
    final entry = parsePairingEntry(
      'excubia://pair?u=http%3A%2F%2F100.64.0.1%3A8788&c=ABCD1234&v=1',
    );

    expect(entry, isA<PairingEntryInvalid>());
    expect((entry as PairingEntryInvalid).reason, contains('ya no se admite'));
  });

  test('text that is no link at all is refused', () {
    expect(parsePairingEntry('hola'), isA<PairingEntryInvalid>());
  });
}
