import 'dart:async';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/connection_failure.dart';
import 'package:herdr_notify/core/api/connection_text.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/api/pinned_client.dart';
import 'package:herdr_notify/core/model/connection_state.dart';
import 'package:herdr_notify/core/state/chat_controller.dart';
import 'package:herdr_notify/core/state/live_triage.dart';
import 'package:herdr_notify/src/rust/api/pinned.dart' as rust;
import 'package:herdr_notify/src/rust/api/signing.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';

import 'support/connection_fixtures.dart';
import 'support/fake_pinned_client.dart';
import 'support/pairing_fixtures.dart';

/// Never answers, so only the client's own deadline can end the call.
class _Silent extends FakePinnedClient {
  @override
  Future<PinnedAnswer> request({
    required List<String> addresses,
    required HostPins pins,
    required String method,
    required String target,
    required Map<String, String> headers,
    required List<int> body,
  }) => Completer<PinnedAnswer>().future;
}

HostClient _pinnedClient(
  PinnedClient pinned, {
  Duration requestTimeout = const Duration(seconds: 30),
}) => HostClient(
  baseUrl: 'https://127.0.0.1:8443',
  deviceId: 'hn-7',
  privateKeyBase64: generateDeviceKey().privateKeyB64,
  pins: HostPins(primary: testPin),
  pinned: pinned,
  requestTimeout: requestTimeout,
);

/// The failure of a call that must fail as a [HostError].
Future<ConnectionFailure?> _failureOf(Future<Object?> call) async {
  try {
    await call;
  } on HostError catch (error) {
    return error.failure;
  }
  fail('the call did not fail');
}

void main() {
  setUpAll(RustLib.init);

  group('a HostError says why the PC could not be reached', () {
    test('a deadline that runs out is a timeout', () async {
      final client = _pinnedClient(
        _Silent(),
        requestTimeout: const Duration(milliseconds: 10),
      );

      expect(await _failureOf(client.state()), ConnectionFailure.timeout);
    });

    test('a pinned timeout from the transport is a timeout', () async {
      final client = _pinnedClient(
        FakePinnedClient(
          failure: const PinnedException(
            rust.PinnedFailureKind.timeout,
            'read timed out',
          ),
        ),
      );

      expect(await _failureOf(client.state()), ConnectionFailure.timeout);
    });

    test('a refused socket is no route', () async {
      final client = await refusedClient();

      expect(await _failureOf(client.state()), ConnectionFailure.noRoute);
    });

    test(
      'a pinned transport that cannot reach any address is no route',
      () async {
        final client = _pinnedClient(
          FakePinnedClient(
            failure: const PinnedException(
              rust.PinnedFailureKind.unreachable,
              'no candidate answered',
            ),
          ),
        );

        expect(await _failureOf(client.state()), ConnectionFailure.noRoute);
      },
    );

    test(
      'a pinned transport whose port refuses the connection is host down',
      () async {
        final client = _pinnedClient(
          FakePinnedClient(
            failure: const PinnedException(
              rust.PinnedFailureKind.refused,
              'connection refused',
            ),
          ),
        );

        expect(await _failureOf(client.state()), ConnectionFailure.hostDown);
      },
    );

    test(
      'a 502, 503 or 504 means the host answered and its service is down',
      () async {
        for (final status in [502, 503, 504]) {
          final client = _pinnedClient(
            FakePinnedClient(
              answer: PinnedAnswer(
                status: status,
                body: 'down',
                address: '127.0.0.1:8443',
              ),
            ),
          );

          expect(
            await _failureOf(client.state()),
            ConnectionFailure.hostDown,
            reason: 'status $status',
          );
        }
      },
    );

    test('a key other than the paired one is an identity mismatch', () async {
      final client = _pinnedClient(
        FakePinnedClient(
          failure: const PinnedException(
            rust.PinnedFailureKind.pinMismatch,
            'sha256/AAAA no coincide con las fijadas',
          ),
        ),
      );

      expect(
        await _failureOf(client.state()),
        ConnectionFailure.identityMismatch,
      );
    });

    test('an answer that is not a host failure has no failure', () async {
      final client = _pinnedClient(
        FakePinnedClient(
          answer: const PinnedAnswer(
            status: 404,
            body: '{"detail":"not found"}',
            address: '127.0.0.1:8443',
          ),
        ),
      );

      expect(await _failureOf(client.state()), isNull);
    });

    test('a broken link is not about the PC, so it has no failure', () async {
      final client = _pinnedClient(
        FakePinnedClient(
          failure: const PinnedException(
            rust.PinnedFailureKind.linkExpired,
            'caducado',
          ),
        ),
      );

      expect(await _failureOf(client.state()), isNull);
    });
  });

  group('the triage and the chat carry the failure to the screen', () {
    test(
      'a refused PC is offline with no route, and the chat says so',
      () async {
        final client = await refusedClient();
        final triage = LiveTriage(client);
        addTearDown(triage.dispose);
        final chat = ChatController(client, triage);
        addTearDown(chat.dispose);

        triage.start();
        final connection = await settled(triage);
        triage.stop();
        expect(triage.connectionFailure, ConnectionFailure.noRoute);
        expect(connection, isA<Offline>());

        await chat.send('hola');

        expect(
          chat.failure?.message,
          connectionText(ConnectionFailure.noRoute),
        );
      },
    );

    test('a host whose service is down is offline with host down', () async {
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      addTearDown(() => server.close(force: true));
      server.listen((request) async {
        request.response.statusCode = 503;
        await request.response.close();
      });
      final triage = LiveTriage(
        HostClient(
          baseUrl: 'http://127.0.0.1:${server.port}',
          deviceId: 'hn-7',
          privateKeyBase64: generateDeviceKey().privateKeyB64,
        ),
      );
      addTearDown(triage.dispose);

      triage.start();
      await settled(triage);
      triage.stop();

      expect(triage.connectionFailure, ConnectionFailure.hostDown);
    });
  });

  group('the sentences and the labels', () {
    test('each failure has its own sentence', () {
      expect(
        connectionText(ConnectionFailure.hostDown),
        'El PC responde, pero el servicio está parado. Arráncalo en el PC e inténtalo de nuevo.',
      );
      expect(
        connectionText(ConnectionFailure.timeout),
        'El PC tardó demasiado en responder. Vuelve a intentarlo.',
      );
      expect(
        connectionText(ConnectionFailure.identityMismatch),
        'El PC ya no es el que emparejaste. Vuelve a emparejarlo desde el PC.',
      );
    });

    test(
      'each failure has its own short label, and none keeps the old one',
      () {
        expect(connectionLabel(ConnectionFailure.noRoute), 'Sin ruta al PC');
        expect(connectionLabel(ConnectionFailure.hostDown), 'PC sin servicio');
        expect(connectionLabel(ConnectionFailure.timeout), 'Lento');
        expect(
          connectionLabel(ConnectionFailure.identityMismatch),
          'PC distinto',
        );
        expect(connectionLabel(null), 'Sin conexión');
      },
    );
  });
}
