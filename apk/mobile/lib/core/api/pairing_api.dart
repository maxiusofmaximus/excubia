import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:herdr_notify/src/rust/api/pinned.dart' as rust;

import 'pinned_client.dart';
import 'tailnet_cleartext.dart';

/// The body of `POST /v1/pair`, built by hand so the bytes that go out are the
/// bytes the gateway parses.
class PairRequest {
  const PairRequest({
    required this.publicKey,
    this.label,
    this.code,
    this.curve = 'p256',
  });

  final String curve;
  final String publicKey;
  final String? label;
  final String? code;

  String toJson() {
    final buffer = StringBuffer('{"curve":${_jsonString(curve)}');
    buffer.write(',"public_key":${_jsonString(publicKey)}');
    final trimmedLabel = label?.trim();
    if (trimmedLabel != null && trimmedLabel.isNotEmpty) {
      buffer.write(',"label":${_jsonString(trimmedLabel)}');
    }
    final trimmedCode = code?.trim();
    if (trimmedCode != null && trimmedCode.isNotEmpty) {
      buffer.write(',"code":${_jsonString(trimmedCode)}');
    }
    buffer.write('}');
    return buffer.toString();
  }
}

class PairResponse {
  const PairResponse({
    required this.deviceId,
    required this.recoveryCode,
    this.address,
  });

  final String deviceId;

  /// Shown once and only in this response.
  final String? recoveryCode;

  /// The candidate address that answered a pinned pairing.
  final String? address;
}

/// A refused pairing. The Rust core turns [status] into text and an action; a
/// pinned refusal carries its [kind] and the technical [detail] instead.
class PairingException implements Exception {
  const PairingException(this.status, {this.kind, this.detail = ''});

  final int status;
  final rust.PinnedFailureKind? kind;
  final String detail;

  @override
  String toString() => 'pairing failed: $status';
}

/// The one unauthenticated write: it carries the public key of the phone and is
/// answered with the device id the gateway assigned.
class PairingApi {
  PairingApi({HttpClient? http, PinnedClient? pinned})
    : _http = http ?? _defaultClient(),
      _pinned = pinned ?? const RustPinnedClient();

  final HttpClient _http;
  final PinnedClient _pinned;

  /// The pinned pairing: the same request, sent only after the host's key has
  /// matched the link's pins.
  Future<PairResponse> pairPinned({
    required List<String> addresses,
    required HostPins pins,
    required PairRequest request,
  }) async {
    try {
      final answer = await _pinned
          .request(
            addresses: addresses,
            pins: pins,
            method: 'POST',
            target: '/v1/pair',
            headers: const {
              'Accept': 'application/json; charset=utf-8',
              'Content-Type': 'application/json; charset=utf-8',
            },
            body: utf8.encode(request.toJson()),
          )
          .timeout(const Duration(seconds: 30));
      if (answer.status != 200) throw PairingException(answer.status);
      final parsed = _parseResponse(answer.body);
      if (parsed == null) throw const PairingException(200);
      return PairResponse(
        deviceId: parsed.deviceId,
        recoveryCode: parsed.recoveryCode,
        address: answer.address,
      );
    } on PinnedException catch (e) {
      throw PairingException(e.status, kind: e.kind, detail: e.detail);
    } on TimeoutException {
      throw const PairingException(0);
    }
  }

  Future<PairResponse> pair(String baseUrl, PairRequest request) async {
    final uri = Uri.parse('${baseUrl.replaceAll(RegExp(r'/+$'), '')}/v1/pair');
    if (!TailnetCleartext.isCleartextAllowed(uri)) {
      throw PairingException(0);
    }
    final body = utf8.encode(request.toJson());
    try {
      final httpRequest = await _http.postUrl(uri);
      httpRequest.headers.set(
        HttpHeaders.acceptHeader,
        'application/json; charset=utf-8',
      );
      httpRequest.headers.contentType = ContentType(
        'application',
        'json',
        charset: 'utf-8',
      );
      httpRequest.add(body);
      final response = await httpRequest.close().timeout(
        const Duration(seconds: 30),
      );
      final text = await response.transform(utf8.decoder).join();
      if (response.statusCode != 200) {
        throw PairingException(response.statusCode);
      }
      final parsed = _parseResponse(text);
      if (parsed == null) {
        throw const PairingException(200);
      }
      return parsed;
    } on PairingException {
      rethrow;
    } on TimeoutException {
      throw const PairingException(0);
    } on SocketException {
      throw const PairingException(0);
    } on HttpException {
      throw const PairingException(0);
    }
  }

  static PairResponse? _parseResponse(String text) {
    try {
      final decoded = jsonDecode(text);
      if (decoded is! Map<String, dynamic>) return null;
      if (decoded['ok'] != true) return null;
      final deviceId = decoded['device_id'];
      if (deviceId is! String || deviceId.isEmpty) return null;
      return PairResponse(
        deviceId: deviceId,
        recoveryCode: decoded['recovery_code'] is String
            ? decoded['recovery_code'] as String
            : null,
      );
    } on FormatException {
      return null;
    }
  }

  static HttpClient _defaultClient() => HttpClient()
    ..connectionTimeout = const Duration(seconds: 10)
    ..idleTimeout = const Duration(minutes: 5)
    ..userAgent = 'herdr-notify-flutter/0.1';
}

String _jsonString(String value) {
  final buffer = StringBuffer('"');
  for (final rune in value.runes) {
    switch (rune) {
      case 0x22:
        buffer.write(r'\"');
      case 0x5c:
        buffer.write(r'\\');
      case 0x0a:
        buffer.write(r'\n');
      case 0x0d:
        buffer.write(r'\r');
      case 0x09:
        buffer.write(r'\t');
      case 0x08:
        buffer.write(r'\b');
      case 0x0c:
        buffer.write(r'\f');
      default:
        if (rune < 0x20) {
          buffer.write('\\u${rune.toRadixString(16).padLeft(4, '0')}');
        } else {
          buffer.writeCharCode(rune);
        }
    }
  }
  buffer.write('"');
  return buffer.toString();
}
