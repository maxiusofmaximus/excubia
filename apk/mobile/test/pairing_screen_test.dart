import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/pairing_api.dart';
import 'package:herdr_notify/core/api/pinned_client.dart';
import 'package:herdr_notify/core/storage/pairing_store.dart';
import 'package:herdr_notify/src/rust/api/pinned.dart' as rust;
import 'package:herdr_notify/src/rust/api/signing.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/pairing_screen.dart';
import 'package:herdr_notify/ui/theme.dart';
import 'package:herdr_notify/ui/widgets/pairing_error_box.dart';

import 'support/fake_pinned_client.dart';
import 'support/memory_store.dart';
import 'support/pairing_fixtures.dart';

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  Widget subject(
    PairingStore store, {
    PairingApi? api,
    ValueChanged<Pairing>? onPaired,
  }) => MaterialApp(
    theme: buildTheme(),
    home: PairingScreen(store: store, onPaired: onPaired ?? (_) {}, api: api),
  );

  testWidgets('a valid pinned link pairs over the pinned transport', (
    tester,
  ) async {
    final fake = FakePinnedClient(
      answer: const PinnedAnswer(
        status: 200,
        body: '{"ok":true,"device_id":"hn-7"}',
        address: '127.0.0.1:8443',
      ),
    );
    Pairing? paired;
    await tester.pumpWidget(
      subject(
        PairingStore(MemoryKeyValueStore()),
        api: PairingApi(pinned: fake),
        onPaired: (pairing) {
          paired = pairing;
        },
      ),
    );
    await tester.pumpAndSettle();

    await pasteLink(tester, pinnedLink(expiresAt: nowSeconds() + 600));

    expect(paired?.deviceId, 'hn-7');
    expect(paired?.baseUrl, 'https://127.0.0.1:8443');
    expect(paired?.pins?.primary, testPin);
    expect(fake.targets, ['/v1/pair']);
    expect(
      find.text(
        'Conexión cifrada (TLS 1.3), verificada con la huella del código.',
      ),
      findsOneWidget,
    );
  });

  testWidgets('a pin mismatch names the danger and folds the detail', (
    tester,
  ) async {
    final fake = FakePinnedClient(
      failure: const PinnedException(
        rust.PinnedFailureKind.pinMismatch,
        'sha256/AAAA no coincide con las fijadas',
      ),
    );
    Pairing? paired;
    await tester.pumpWidget(
      subject(
        PairingStore(MemoryKeyValueStore()),
        api: PairingApi(pinned: fake),
        onPaired: (pairing) {
          paired = pairing;
        },
      ),
    );
    await tester.pumpAndSettle();

    await pasteLink(tester, pinnedLink(expiresAt: nowSeconds() + 600));

    expect(paired, isNull);
    expect(
      find.text('La huella no coincide: no te conectes a esta red'),
      findsOneWidget,
    );
    expect(find.text('Detalle técnico'), findsOneWidget);
    expect(
      find.text(
        'Conexión cifrada (TLS 1.3), verificada con la huella del código.',
      ),
      findsNothing,
    );
  });

  testWidgets('an expired link says so and never reaches the host', (
    tester,
  ) async {
    final fake = FakePinnedClient();
    await tester.pumpWidget(
      subject(
        PairingStore(MemoryKeyValueStore()),
        api: PairingApi(pinned: fake),
      ),
    );
    await tester.pumpAndSettle();

    await pasteLink(tester, pinnedLink(expiresAt: nowSeconds() - 60));

    expect(
      find.text('El código ha caducado. Genera uno nuevo en el equipo.'),
      findsOneWidget,
    );
    expect(fake.targets, isEmpty);
  });

  testWidgets('pasted text that is no link is refused on the screen', (
    tester,
  ) async {
    final fake = FakePinnedClient();
    await tester.pumpWidget(
      subject(
        PairingStore(MemoryKeyValueStore()),
        api: PairingApi(pinned: fake),
      ),
    );
    await tester.pumpAndSettle();

    await pasteLink(tester, 'hola');

    expect(find.byType(PairingErrorBox), findsOneWidget);
    expect(fake.targets, isEmpty);
  });

  testWidgets('the pairing-code card is the primary path', (tester) async {
    final store = PairingStore(MemoryKeyValueStore());
    await tester.pumpWidget(subject(store));
    await tester.pumpAndSettle();

    expect(find.text('Código de emparejamiento'), findsOneWidget);
    expect(
      find.text('Déjalo vacío para que la app busque el equipo sola.'),
      findsOneWidget,
    );
    expect(find.text('Escanear QR'), findsOneWidget);
    expect(find.text('Avanzado'), findsOneWidget);
    // The manual fields live inside the collapsed advanced block.
    expect(find.text('URL del gateway'), findsNothing);
  });

  testWidgets('pairing without an address says what is missing', (
    tester,
  ) async {
    final store = PairingStore(MemoryKeyValueStore());
    await tester.pumpWidget(subject(store));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Emparejar'));
    await tester.pumpAndSettle();

    expect(
      find.textContaining('Añade la dirección del equipo'),
      findsOneWidget,
    );
  });

  testWidgets('a generated key is persisted and the Rust core is shown', (
    tester,
  ) async {
    final memory = MemoryKeyValueStore();
    final store = PairingStore(memory);
    await tester.pumpWidget(subject(store));
    await tester.pumpAndSettle();

    final loaded = await store.load();
    expect(publicKeyFromPrivate(privateKeyB64: loaded.privateKey), isNotNull);

    await tester.tap(find.text('Avanzado'));
    await tester.pumpAndSettle();
    expect(find.textContaining('Núcleo Rust'), findsOneWidget);
  });
}
