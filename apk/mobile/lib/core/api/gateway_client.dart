import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:herdr_notify/src/rust/api/signing.dart';

import '../storage/pairing_store.dart';
import 'chat_transport.dart';
import 'connection_failure.dart';
import 'pinned_client.dart';
import 'tailnet_cleartext.dart';

/// The signed HTTP client. Headers and signatures come from the Rust core;
/// this class owns the socket, the URL and the status handling. A host paired
/// through a pinned link is reached only through [PinnedClient].
class GatewayClient implements ChatTransport {
  GatewayClient({
    required this.baseUrl,
    required this.deviceId,
    required this.privateKeyBase64,
    this.pins,
    PinnedClient? pinned,
    HttpClient? http,
    this.requestTimeout = const Duration(seconds: 30),
  }) : _http = http ?? _defaultClient(),
       _pinned = pinned ?? const RustPinnedClient();

  /// The client of a paired host: its address, the device key and its pins.
  factory GatewayClient.forPairing(Pairing pairing) => GatewayClient(
    baseUrl: pairing.baseUrl,
    deviceId: pairing.deviceId,
    privateKeyBase64: pairing.privateKey,
    pins: pairing.pins,
  );

  final String baseUrl;
  final String deviceId;
  final String privateKeyBase64;

  /// Set when the host was paired through a link that carried a pin.
  final HostPins? pins;
  final Duration requestTimeout;
  final HttpClient _http;
  final PinnedClient _pinned;

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) => _exchange(method, target, body);

  /// A raw body under its own `Content-Type`; the signature covers the bytes.
  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      _exchange('POST', target, body, contentType: contentType);

  /// The bytes of a binary answer, as the host sent them.
  @override
  Future<List<int>> callBytes(String method, String target) async {
    final headers = _headers(_sign(method, target, const []));
    final pins = this.pins;
    if (pins != null) return _callPinnedBytes(pins, method, target, headers);
    return _sendTimed(method, target, headers, const []);
  }

  Future<String> _exchange(
    String method,
    String target,
    List<int> body, {
    String? contentType,
  }) async {
    final headers = _headers(
      _sign(method, target, body),
      contentType: contentType,
    );
    final pins = this.pins;
    if (pins != null) return _callPinned(pins, method, target, headers, body);
    return utf8.decode(await _sendTimed(method, target, headers, body));
  }

  Future<String> state() => call('GET', '/v1/state');

  /// `GET /v1/events` as a line stream, for the Rust decoder. [lastEventId] resumes after the
  /// last `id:` seen; the signature covers method, target and body only, so it is not signed.
  Future<Stream<String>> eventLines({String? lastEventId}) async {
    final headers = {
      ..._headers(_sign('GET', '/v1/events', const [])),
      'Last-Event-ID': ?lastEventId,
    };
    final pins = this.pins;
    if (pins != null) {
      return _pinned
          .eventLines(
            addresses: [_authority()],
            pins: pins,
            target: '/v1/events',
            headers: headers,
          )
          .handleError(
            (Object error) => throw _gatewayErrorOf(error as PinnedException),
            test: (error) => error is PinnedException,
          );
    }
    final request = await _openRequest('GET', _uriFor('/v1/events'), headers);
    request.headers.set(HttpHeaders.acceptHeader, 'text/event-stream');
    request.headers.set(HttpHeaders.cacheControlHeader, 'no-store');
    final response = await request.close();
    if (response.statusCode != 200) {
      final text = await response.transform(utf8.decoder).join();
      throw GatewayError(
        response.statusCode,
        text,
        retryAfter: _retryAfter(response.headers),
      );
    }
    return response.transform(utf8.decoder).transform(const LineSplitter());
  }

  Future<String> _callPinned(
    HostPins pins,
    String method,
    String target,
    Map<String, String> headers,
    List<int> body,
  ) async {
    final answer = await _guardPinned(
      _pinned.request(
        addresses: [_authority()],
        pins: pins,
        method: method,
        target: target,
        headers: headers,
        body: body,
      ),
    );
    if (answer.status < 200 || answer.status >= 300) {
      throw GatewayError(
        answer.status,
        answer.body,
        retryAfter: answer.retryAfter,
      );
    }
    return answer.body;
  }

  Future<List<int>> _callPinnedBytes(
    HostPins pins,
    String method,
    String target,
    Map<String, String> headers,
  ) async {
    final answer = await _guardPinned(
      _pinned.requestBytes(
        addresses: [_authority()],
        pins: pins,
        method: method,
        target: target,
        headers: headers,
        body: const [],
      ),
    );
    if (answer.status < 200 || answer.status >= 300) {
      throw GatewayError(
        answer.status,
        utf8.decode(answer.bytes, allowMalformed: true),
        retryAfter: answer.retryAfter,
      );
    }
    return answer.bytes;
  }

  /// Runs one pinned exchange under the deadline, failures as [GatewayError].
  Future<T> _guardPinned<T>(Future<T> exchange) async {
    try {
      return await exchange.timeout(requestTimeout);
    } on PinnedException catch (e) {
      throw _gatewayErrorOf(e);
    } on TimeoutException {
      throw const GatewayError.transport(
        ConnectionFailure.timeout,
        'el equipo no respondió a tiempo',
      );
    }
  }

  SignedHeaders _sign(String method, String target, List<int> body) {
    final signed = signRequest(
      privateKeyB64: privateKeyBase64,
      method: method,
      target: target,
      body: body,
    );
    if (signed == null) {
      throw StateError('La clave del dispositivo no es válida.');
    }
    return signed;
  }

  /// Sends on the plain socket, with the deadline and the socket errors as
  /// [GatewayError]s.
  Future<List<int>> _sendTimed(
    String method,
    String target,
    Map<String, String> headers,
    List<int> body,
  ) async {
    try {
      return await _send(
        method,
        _uriFor(target),
        headers,
        body,
      ).timeout(requestTimeout);
    } on GatewayError {
      rethrow;
    } on TimeoutException {
      throw const GatewayError.transport(
        ConnectionFailure.timeout,
        'el equipo no respondió a tiempo',
      );
    } on SocketException catch (e) {
      throw GatewayError.transport(ConnectionFailure.noRoute, e.message);
    } on HttpException catch (e) {
      throw GatewayError.transport(ConnectionFailure.noRoute, e.message);
    }
  }

  Future<List<int>> _send(
    String method,
    Uri uri,
    Map<String, String> headers,
    List<int> body,
  ) async {
    final request = await _openRequest(method, uri, headers);
    if (body.isNotEmpty) request.add(body);
    final response = await request.close();
    final bytes = await response.expand((chunk) => chunk).toList();
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw GatewayError(
        response.statusCode,
        utf8.decode(bytes, allowMalformed: true),
        retryAfter: _retryAfter(response.headers),
      );
    }
    return bytes;
  }

  static int? _retryAfter(HttpHeaders headers) {
    final raw = headers.value(HttpHeaders.retryAfterHeader);
    return raw == null ? null : int.tryParse(raw.trim());
  }

  Future<HttpClientRequest> _openRequest(
    String method,
    Uri uri,
    Map<String, String> headers,
  ) async {
    if (!TailnetCleartext.isCleartextAllowed(uri)) {
      throw GatewayError(0, 'HTTP en claro no permitido para ${uri.host}');
    }
    final request = await _http.openUrl(method.toUpperCase(), uri);
    headers.forEach(request.headers.set);
    return request;
  }

  Map<String, String> _headers(SignedHeaders signed, {String? contentType}) => {
    HttpHeaders.acceptHeader: 'application/json; charset=utf-8',
    'X-HN-Device': deviceId,
    'X-HN-Timestamp': signed.timestamp.toString(),
    'X-HN-Nonce': signed.nonce,
    'X-HN-Signature': signed.signature,
    HttpHeaders.contentTypeHeader: ?contentType,
  };

  /// `host:port` of the base URL, the form the pinned transport dials.
  String _authority() {
    final uri = Uri.parse(baseUrl);
    final host = uri.host.contains(':') ? '[${uri.host}]' : uri.host;
    return '$host:${uri.port}';
  }

  Uri _uriFor(String target) =>
      Uri.parse('${baseUrl.replaceAll(RegExp(r'/+$'), '')}$target');

  static GatewayError _gatewayErrorOf(PinnedException error) =>
      error.status != 0
      ? GatewayError(error.status, error.detail, retryAfter: error.retryAfter)
      : GatewayError.transport(
          connectionFailureOfPinned(error.kind),
          error.message,
        );

  static HttpClient _defaultClient() => HttpClient()
    ..connectionTimeout = const Duration(seconds: 10)
    ..idleTimeout = const Duration(minutes: 5)
    ..userAgent = 'herdr-notify-flutter/0.1';
}

/// A refused request: [status] 0 means the transport failed before a response.
class GatewayError implements Exception {
  const GatewayError(this.status, this.body, {this.retryAfter})
    : _transport = null;

  /// A transport failure. Its status is 0, so the callers that test it still work.
  const GatewayError.transport(ConnectionFailure? failure, this.body)
    : status = 0,
      retryAfter = null,
      _transport = failure;

  final int status;
  final String body;

  /// `Retry-After` in seconds, when the host sent it.
  final int? retryAfter;

  final ConnectionFailure? _transport;

  /// Why the PC could not be reached. Null when the host answered with a status
  /// that is not a gateway failure, or when the error is not about the link.
  ConnectionFailure? get failure =>
      _transport ??
      (status == 502 || status == 503 || status == 504
          ? ConnectionFailure.hostDown
          : null);

  @override
  String toString() => '$status: $body';
}
