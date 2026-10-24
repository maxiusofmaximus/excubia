import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:herdr_notify/src/rust/api/signing.dart';

import 'chat_transport.dart';
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
  ]) async {
    final signed = _sign(method, target, body);
    final pins = this.pins;
    if (pins != null) return _callPinned(pins, method, target, signed, body);
    try {
      return await _send(
        method,
        _uriFor(target),
        signed,
        body,
      ).timeout(requestTimeout);
    } on GatewayError {
      rethrow;
    } on TimeoutException {
      throw const GatewayError(0, 'el equipo no respondió a tiempo');
    } on SocketException catch (e) {
      throw GatewayError(0, e.message);
    } on HttpException catch (e) {
      throw GatewayError(0, e.message);
    }
  }

  Future<String> state() => call('GET', '/v1/state');

  /// `GET /v1/events` as a line stream, for the Rust decoder.
  Future<Stream<String>> eventLines() async {
    final signed = _sign('GET', '/v1/events', const []);
    final pins = this.pins;
    if (pins != null) {
      return _pinned
          .eventLines(
            addresses: [_authority()],
            pins: pins,
            target: '/v1/events',
            headers: _headers(signed),
          )
          .handleError(
            (Object error) => throw _gatewayErrorOf(error as PinnedException),
            test: (error) => error is PinnedException,
          );
    }
    final request = await _openRequest('GET', _uriFor('/v1/events'), signed);
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
    SignedHeaders signed,
    List<int> body,
  ) async {
    try {
      final answer = await _pinned
          .request(
            addresses: [_authority()],
            pins: pins,
            method: method,
            target: target,
            headers: _headers(signed),
            body: body,
          )
          .timeout(requestTimeout);
      if (answer.status < 200 || answer.status >= 300) {
        throw GatewayError(
          answer.status,
          answer.body,
          retryAfter: answer.retryAfter,
        );
      }
      return answer.body;
    } on PinnedException catch (e) {
      throw _gatewayErrorOf(e);
    } on TimeoutException {
      throw const GatewayError(0, 'el equipo no respondió a tiempo');
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

  Future<String> _send(
    String method,
    Uri uri,
    SignedHeaders signed,
    List<int> body,
  ) async {
    final request = await _openRequest(method, uri, signed);
    if (body.isNotEmpty) request.add(body);
    final response = await request.close();
    final text = await response.transform(utf8.decoder).join();
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw GatewayError(
        response.statusCode,
        text,
        retryAfter: _retryAfter(response.headers),
      );
    }
    return text;
  }

  static int? _retryAfter(HttpHeaders headers) {
    final raw = headers.value(HttpHeaders.retryAfterHeader);
    return raw == null ? null : int.tryParse(raw.trim());
  }

  Future<HttpClientRequest> _openRequest(
    String method,
    Uri uri,
    SignedHeaders signed,
  ) async {
    if (!TailnetCleartext.isCleartextAllowed(uri)) {
      throw GatewayError(0, 'HTTP en claro no permitido para ${uri.host}');
    }
    final request = await _http.openUrl(method.toUpperCase(), uri);
    _headers(signed).forEach(request.headers.set);
    return request;
  }

  Map<String, String> _headers(SignedHeaders signed) => {
    HttpHeaders.acceptHeader: 'application/json; charset=utf-8',
    'X-HN-Device': deviceId,
    'X-HN-Timestamp': signed.timestamp.toString(),
    'X-HN-Nonce': signed.nonce,
    'X-HN-Signature': signed.signature,
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
      : GatewayError(0, error.message);

  static HttpClient _defaultClient() => HttpClient()
    ..connectionTimeout = const Duration(seconds: 10)
    ..idleTimeout = const Duration(minutes: 5)
    ..userAgent = 'herdr-notify-flutter/0.1';
}

/// A refused request: [status] 0 means the transport failed before a response.
class GatewayError implements Exception {
  const GatewayError(this.status, this.body, {this.retryAfter});

  final int status;
  final String body;

  /// `Retry-After` in seconds, when the host sent it.
  final int? retryAfter;

  @override
  String toString() => '$status: $body';
}
