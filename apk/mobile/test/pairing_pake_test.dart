import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/pair_discovery.dart';
import 'package:herdr_notify/core/api/pairing_api.dart';
import 'package:herdr_notify/core/api/pake_pairing.dart';
import 'package:herdr_notify/core/storage/pairing_store.dart';
import 'package:herdr_notify/src/rust/api/pinned.dart' as rust;
import 'package:herdr_notify/src/rust/api/signing.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/pairing_screen.dart';
import 'package:herdr_notify/ui/theme.dart';
import 'package:herdr_notify/ui/widgets/pairing_error_box.dart';

import 'support/fake_pair_discovery.dart';
import 'support/fake_pinned_client.dart';
import 'support/memory_store.dart';
import 'support/pairing_fixtures.dart';

const _code = 'KQ7M2XW9';
const _found = '192.168.1.20:47443';

PakePairing _paired() => PakePaired(
  deviceId: 'hn-9',
  recoveryCode: 'RECO-VERY-0001',
  pin: testPin,
  address: _found,
);

PakePairing _refused(
  rust.PakeFailureKind kind, {
  String message = 'El equipo no lo aceptó.',
  int? retryAfter,
}) => PakeRefused(kind, message, retryAfter: retryAfter);

/// A discovery that answers when the test says so.
class _SlowDiscovery implements PairDiscovery {
  final Completer<List<String>> reply = Completer();

  @override
  Future<List<String>> discover({
    Duration timeout = const Duration(seconds: 15),
  }) => reply.future;
}

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  late MemoryKeyValueStore memory;
  late PairingStore store;
  Pairing? paired;

  setUp(() {
    memory = MemoryKeyValueStore();
    store = PairingStore(memory);
    paired = null;
  });

  Future<void> mount(
    WidgetTester tester,
    FakePinnedClient pinned,
    PairDiscovery discovery,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: PairingScreen(
          store: store,
          onPaired: (pairing) => paired = pairing,
          api: PairingApi(pinned: pinned),
          discovery: discovery,
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  Future<void> typeCode(WidgetTester tester, String code) async {
    await tester.enterText(find.byType(TextField).first, code);
  }

  Future<void> typeAddress(WidgetTester tester, String address) async {
    await tester.tap(find.text('Avanzado'));
    await tester.pumpAndSettle();
    final field = find.widgetWithText(TextField, 'Dirección del PC');
    await tester.ensureVisible(field);
    await tester.enterText(field, address);
    // The list builds lazily: bring the pairing button back into view.
    await tester.drag(find.byType(ListView), const Offset(0, 2000));
    await tester.pumpAndSettle();
  }

  Future<void> press(WidgetTester tester) async {
    await tester.ensureVisible(find.text('Emparejar'));
    await tester.tap(find.text('Emparejar'));
    await tester.pumpAndSettle();
  }

  String codeField(WidgetTester tester) =>
      tester.widget<TextField>(find.byType(TextField).first).controller!.text;

  /// No text on the screen may carry the code, whatever the outcome.
  void expectCodeNowhere(WidgetTester tester) {
    for (final text in tester.widgetList<Text>(find.byType(Text))) {
      final shown = text.data ?? text.textSpan?.toPlainText() ?? '';
      expect(shown.toUpperCase(), isNot(contains(_code)));
    }
  }

  testWidgets('8 characters with an empty address discover and pair by PAKE', (
    tester,
  ) async {
    final pinned = FakePinnedClient(pake: _paired());
    final discovery = FakePairDiscovery(found: const [_found, '10.0.0.5:1']);
    await mount(tester, pinned, discovery);

    await typeCode(tester, 'kq7m-2xw9');
    await press(tester);

    expect(discovery.timeouts, hasLength(1));
    expect(pinned.pakeCalls, hasLength(1));
    expect(pinned.pakeCalls.single.code, _code);
    expect(pinned.pakeCalls.single.addresses, [_found, '10.0.0.5:1']);
    expect(pinned.targets, isEmpty);
    final key = publicKeyFromPrivate(
      privateKeyB64: (await store.load()).privateKey,
    )!;
    expect(pinned.pakeCalls.single.key, key);
  });

  testWidgets('success saves the device and shows the string to compare', (
    tester,
  ) async {
    final pinned = FakePinnedClient(pake: _paired());
    await mount(tester, pinned, FakePairDiscovery(found: const [_found]));

    await typeCode(tester, _code);
    await press(tester);

    final key = publicKeyFromPrivate(
      privateKeyB64: (await store.load()).privateKey,
    )!;
    final sas = rust.pairingSas(pin: testPin, publicKeyBase64: key)!;
    expect(find.text('Comprueba esta cadena'), findsOneWidget);
    expect(
      find.text('${sas.substring(0, 3)} ${sas.substring(3)}'),
      findsOneWidget,
    );
    expect(find.textContaining('El PC debe aprobar'), findsOneWidget);
    expect(paired, isNull);

    await tester.tap(find.text('Entendido'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Hecho'));
    await tester.pumpAndSettle();

    expect(paired?.deviceId, 'hn-9');
    expect(paired?.pins?.primary, testPin);
    expect(paired?.baseUrl, 'https://$_found');
    final saved = await store.load();
    expect(saved.deviceId, 'hn-9');
    expect(saved.pins?.primary, testPin);
    expect(memory.values.values, contains('RECO-VERY-0001'));
  });

  testWidgets('the screen says it is looking and then pairing', (tester) async {
    final discovery = _SlowDiscovery();
    final pinned = FakePinnedClient(
      pake: _refused(rust.PakeFailureKind.noPake),
    );
    await mount(tester, pinned, discovery);

    await typeCode(tester, _code);
    await tester.ensureVisible(find.text('Emparejar'));
    await tester.tap(find.text('Emparejar'));
    await tester.pump();
    expect(find.text('Buscando el PC en la red…'), findsOneWidget);

    discovery.reply.complete(const [_found]);
    await tester.pumpAndSettle();
    expect(find.text('Buscando el PC en la red…'), findsNothing);
    expect(pinned.pakeCalls, hasLength(1));
  });

  testWidgets('a typed address is the only candidate and skips discovery', (
    tester,
  ) async {
    final pinned = FakePinnedClient(
      pake: _refused(rust.PakeFailureKind.noPake),
    );
    final discovery = FakePairDiscovery(found: const ['10.9.9.9:1']);
    await mount(tester, pinned, discovery);

    await typeAddress(tester, '192.168.1.77:47443');
    await typeCode(tester, _code);
    await press(tester);

    expect(discovery.timeouts, isEmpty);
    expect(pinned.pakeCalls.single.addresses, ['192.168.1.77:47443']);
  });

  testWidgets('7 characters keep the F1 path and never reach the PAKE', (
    tester,
  ) async {
    final pinned = FakePinnedClient(pake: _paired());
    final discovery = FakePairDiscovery(found: const [_found]);
    await mount(tester, pinned, discovery);

    await typeCode(tester, 'KQ7M2XW');
    await press(tester);

    expect(pinned.pakeCalls, isEmpty);
    expect(discovery.timeouts, isEmpty);
    expect(
      find.textContaining('Añade la dirección del equipo'),
      findsOneWidget,
    );
  });

  testWidgets('no PC found says so, keeps the code and sends nothing', (
    tester,
  ) async {
    final pinned = FakePinnedClient(pake: _paired());
    await mount(tester, pinned, FakePairDiscovery());

    await typeCode(tester, _code);
    await press(tester);

    expect(find.textContaining('No encontré el PC'), findsOneWidget);
    expect(pinned.pakeCalls, isEmpty);
    expect(codeField(tester), _code);
  });

  testWidgets('the error sits under the code card, not off-screen below', (
    tester,
  ) async {
    await mount(tester, FakePinnedClient(pake: _paired()), FakePairDiscovery());

    await typeCode(tester, _code);
    await press(tester);

    final error = tester.getTopLeft(find.byType(PairingErrorBox)).dy;
    final screen =
        tester.view.physicalSize.height / tester.view.devicePixelRatio;
    expect(error, lessThan(tester.getTopLeft(find.text('Escanear QR')).dy));
    expect(error, lessThan(screen));
  });

  testWidgets('a discovery that breaks shows its own message', (tester) async {
    final pinned = FakePinnedClient(pake: _paired());
    await mount(tester, pinned, FakePairDiscovery(fails: true));

    await typeCode(tester, _code);
    await press(tester);

    expect(find.text(const PairDiscoveryException().message), findsOneWidget);
    expect(pinned.pakeCalls, isEmpty);
    expect(codeField(tester), _code);
  });

  for (final kind in [
    rust.PakeFailureKind.hostProofInvalid,
    rust.PakeFailureKind.channelChanged,
  ]) {
    testWidgets('$kind is a security warning and empties the code', (
      tester,
    ) async {
      final pinned = FakePinnedClient(
        pake: _refused(kind, message: 'La prueba del equipo no cuadra.'),
      );
      await mount(tester, pinned, FakePairDiscovery(found: const [_found]));

      await typeCode(tester, _code);
      await press(tester);

      expect(find.text('No se pudo verificar el equipo'), findsOneWidget);
      expect(find.text('La prueba del equipo no cuadra.'), findsOneWidget);
      expect(
        tester.widget<PairingErrorBox>(find.byType(PairingErrorBox)).security,
        isTrue,
      );
      expect(find.byIcon(Icons.gpp_bad_outlined), findsOneWidget);
      expect(codeField(tester), isEmpty);
      expect(paired, isNull);
      expectCodeNowhere(tester);
    });
  }

  testWidgets('a spent code shows the message and empties the field', (
    tester,
  ) async {
    final pinned = FakePinnedClient(
      pake: _refused(
        rust.PakeFailureKind.codeGone,
        message: 'El código caducó. Pide uno nuevo.',
      ),
    );
    await mount(tester, pinned, FakePairDiscovery(found: const [_found]));

    await typeCode(tester, _code);
    await press(tester);

    expect(find.text('El código caducó. Pide uno nuevo.'), findsOneWidget);
    expect(
      tester.widget<PairingErrorBox>(find.byType(PairingErrorBox)).security,
      isFalse,
    );
    expect(codeField(tester), isEmpty);
    expectCodeNowhere(tester);
  });

  testWidgets('rate limiting shows how long to wait', (tester) async {
    final pinned = FakePinnedClient(
      pake: _refused(
        rust.PakeFailureKind.rateLimited,
        message: 'Demasiados intentos.',
        retryAfter: 30,
      ),
    );
    await mount(tester, pinned, FakePairDiscovery(found: const [_found]));

    await typeCode(tester, _code);
    await press(tester);

    expect(
      find.text('Demasiados intentos. Vuelve a intentarlo en 30 s.'),
      findsOneWidget,
    );
    expect(codeField(tester), isEmpty);
  });

  testWidgets('a malformed code stays in the field to be corrected', (
    tester,
  ) async {
    final pinned = FakePinnedClient(
      pake: _refused(
        rust.PakeFailureKind.invalidCodeFormat,
        message: 'Eso no es un código.',
      ),
    );
    await mount(tester, pinned, FakePairDiscovery(found: const [_found]));

    await typeCode(tester, _code);
    await press(tester);

    expect(find.text('Eso no es un código.'), findsOneWidget);
    expect(codeField(tester), _code);
  });

  for (final kind in [
    rust.PakeFailureKind.unreachable,
    rust.PakeFailureKind.noPake,
    rust.PakeFailureKind.protocol,
  ]) {
    testWidgets('$kind shows the message and empties the code', (tester) async {
      final pinned = FakePinnedClient(
        pake: _refused(kind, message: 'Mensaje de ${kind.name}.'),
      );
      await mount(tester, pinned, FakePairDiscovery(found: const [_found]));

      await typeCode(tester, _code);
      await press(tester);

      expect(find.text('Mensaje de ${kind.name}.'), findsOneWidget);
      expect(codeField(tester), isEmpty);
      expectCodeNowhere(tester);
    });
  }

  testWidgets('the card describes the 8-character code', (tester) async {
    await mount(tester, FakePinnedClient(), FakePairDiscovery());

    expect(
      find.text(
        'Escribe el código de 8 caracteres que muestra el PC; '
        'la app buscará el equipo sola.',
      ),
      findsOneWidget,
    );
    expect(find.text('8 caracteres'), findsOneWidget);
  });
}
