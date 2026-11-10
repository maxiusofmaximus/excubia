import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/model/connection_state.dart';
import 'package:excubia/core/state/chat_controller.dart';
import 'package:excubia/core/state/live_triage.dart';
import 'package:excubia/src/rust/api/signing.dart';
import 'package:excubia/src/rust/frb_generated.dart';

/// A host on the loopback address that answers every request with [status].
/// The production client runs unchanged against it.
Future<HttpServer> _host(int status) async {
  final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
  server.listen((request) async {
    request.response
      ..statusCode = status
      ..write('{"detail":"forbidden"}');
    await request.response.close();
  });
  return server;
}

LiveTriage _triageFor(String baseUrl) => LiveTriage(
  HostClient(
    baseUrl: baseUrl,
    deviceId: 'dev-test',
    privateKeyBase64: generateDeviceKey().privateKeyB64,
  ),
);

/// Waits until the triage has said something other than «connecting».
Future<ConnectionState> _settled(LiveTriage triage) async {
  for (var i = 0; i < 500; i++) {
    final connection = triage.connection;
    if (connection is! Connecting) return connection;
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
  fail('the triage stayed in Connecting');
}

void main() {
  setUpAll(RustLib.init);

  group('a paired device without read_state', () {
    test('is awaiting approval, not offline', () async {
      final server = await _host(403);
      addTearDown(() => server.close(force: true));
      final triage = _triageFor('http://127.0.0.1:${server.port}');
      addTearDown(triage.dispose);

      triage.start();

      expect(await _settled(triage), isA<AwaitingApproval>());
    });

    test('a refresh that gets the same 403 also says awaiting approval', () async {
      final server = await _host(403);
      addTearDown(() => server.close(force: true));
      final triage = _triageFor('http://127.0.0.1:${server.port}');
      addTearDown(triage.dispose);

      await triage.refreshNow();

      expect(triage.connection, isA<AwaitingApproval>());
    });
  });

  group('the chat while awaiting approval', () {
    test('cannot send, and says it waits for the approval', () async {
      final server = await _host(403);
      addTearDown(() => server.close(force: true));
      final client = HostClient(
        baseUrl: 'http://127.0.0.1:${server.port}',
        deviceId: 'dev-test',
        privateKeyBase64: generateDeviceKey().privateKeyB64,
      );
      final triage = LiveTriage(client);
      addTearDown(triage.dispose);
      final chat = ChatController(client, triage);
      addTearDown(chat.dispose);

      triage.start();
      await _settled(triage);

      expect(chat.awaitingApproval, isTrue);
      expect(chat.offline, isFalse);
      expect(chat.canSend, isFalse);
    });
  });

  group('a host that cannot be reached', () {
    test('is offline, not awaiting approval', () async {
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      final port = server.port;
      await server.close(force: true);
      final triage = _triageFor('http://127.0.0.1:$port');
      addTearDown(triage.dispose);

      triage.start();
      final connection = await _settled(triage);
      triage.stop();

      expect(connection, isNot(isA<AwaitingApproval>()));
      expect(connection, anyOf(isA<Offline>(), isA<Failed>()));
    });
  });
}
