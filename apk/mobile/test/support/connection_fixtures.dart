import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/model/connection_state.dart';
import 'package:excubia/core/state/live_triage.dart';
import 'package:excubia/src/rust/api/signing.dart';

/// A client for a loopback port with nothing listening on it: the socket is refused.
Future<HostClient> refusedClient() async {
  final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
  final port = server.port;
  await server.close(force: true);
  return HostClient(
    baseUrl: 'http://127.0.0.1:$port',
    deviceId: 'hn-7',
    privateKeyBase64: generateDeviceKey().privateKeyB64,
  );
}

/// Every socket is refused without touching the network. The widget test
/// binding answers real HTTP with a 400, so a widget test needs this instead.
HostClient refusingClient() => HostClient(
  baseUrl: 'http://127.0.0.1:1',
  deviceId: 'hn-7',
  privateKeyBase64: generateDeviceKey().privateKeyB64,
  http: _RefusingHttpClient(),
);

class _RefusingHttpClient implements HttpClient {
  @override
  Future<HttpClientRequest> openUrl(String method, Uri url) async =>
      throw const SocketException('Connection refused');

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

/// Waits until the triage has said something other than «connecting».
Future<ConnectionState> settled(LiveTriage triage) async {
  for (var i = 0; i < 500; i++) {
    final connection = triage.connection;
    if (connection is! Connecting) return connection;
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
  fail('the triage stayed in Connecting');
}
