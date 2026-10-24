import 'package:herdr_notify/src/rust/api/pinned.dart' as rust;

/// The keys a paired host must present, taken from its pairing link. [backup]
/// is set only while a rotation is prepared; either key is accepted.
class HostPins {
  const HostPins({required this.primary, this.backup});

  final String primary;
  final String? backup;
}

/// One answered request. The status belongs to the host, whatever it is.
class PinnedAnswer {
  const PinnedAnswer({
    required this.status,
    required this.body,
    required this.address,
    this.retryAfter,
  });

  final int status;
  final String body;

  /// The candidate address that answered, so the caller can keep it.
  final String address;
  final int? retryAfter;
}

/// A pinned exchange that did not complete. [kind] is what the UI branches on;
/// [detail] is the technical text, shown folded.
class PinnedException implements Exception {
  const PinnedException(
    this.kind,
    this.detail, {
    this.status = 0,
    this.retryAfter,
  });

  final rust.PinnedFailureKind kind;
  final String detail;
  final int status;
  final int? retryAfter;

  String get message => pinnedFailureMessage(kind);

  @override
  String toString() => 'pinned ${kind.name}: $detail';
}

/// The sentence the phone shows for a pinned failure, in Spanish. Shared by the
/// request path and the pairing screen so one kind never reads two ways.
String pinnedFailureMessage(rust.PinnedFailureKind kind) => switch (kind) {
  rust.PinnedFailureKind.pinMismatch =>
    'La huella no coincide: no te conectes a esta red',
  rust.PinnedFailureKind.certificateInvalid => 'El certificado del equipo ya no es válido. Vuelve a emparejar desde el equipo.',
  rust.PinnedFailureKind.linkExpired => 'El código ha caducado',
  rust.PinnedFailureKind.timeout => 'el equipo no respondió a tiempo',
  rust.PinnedFailureKind.unreachable =>
    'no se pudo llegar al equipo; ¿está en la misma red?',
  rust.PinnedFailureKind.protocol =>
    'el equipo no respondió con una conexión cifrada válida',
  rust.PinnedFailureKind.rejected => 'el equipo rechazó la petición',
  _ => 'el enlace de emparejamiento no es válido',
};

/// The TLS 1.3 transport for a host whose link carried a pin. The Rust core
/// verifies the host's key against [HostPins] before a request byte is written.
abstract interface class PinnedClient {
  Future<PinnedAnswer> request({
    required List<String> addresses,
    required HostPins pins,
    required String method,
    required String target,
    required Map<String, String> headers,
    required List<int> body,
  });

  Stream<String> eventLines({
    required List<String> addresses,
    required HostPins pins,
    required String target,
    required Map<String, String> headers,
  });
}

/// The production transport: the shared Rust core does the handshake and the
/// pin check, Dart only hands it the addresses and the signed headers.
class RustPinnedClient implements PinnedClient {
  const RustPinnedClient();

  @override
  Future<PinnedAnswer> request({
    required List<String> addresses,
    required HostPins pins,
    required String method,
    required String target,
    required Map<String, String> headers,
    required List<int> body,
  }) async {
    final reply = await rust.pinnedRequest(
      endpoint: _endpoint(addresses, pins),
      method: method,
      target: target,
      headers: _fields(headers),
      body: body,
    );
    final response = reply.response;
    if (response == null) throw _failed(reply.failure);
    return PinnedAnswer(
      status: response.status,
      body: response.body,
      address: response.address,
      retryAfter: response.retryAfter,
    );
  }

  @override
  Stream<String> eventLines({
    required List<String> addresses,
    required HostPins pins,
    required String target,
    required Map<String, String> headers,
  }) async* {
    final lines = rust.pinnedEventLines(
      endpoint: _endpoint(addresses, pins),
      target: target,
      headers: _fields(headers),
    );
    await for (final line in lines) {
      final text = line.text;
      if (text != null) {
        yield text;
      } else {
        throw _failed(line.failure);
      }
    }
  }

  static rust.PinnedEndpoint _endpoint(List<String> addresses, HostPins pins) =>
      rust.PinnedEndpoint(
        addresses: addresses,
        primary: pins.primary,
        backup: pins.backup,
      );

  static List<rust.HeaderField> _fields(Map<String, String> headers) => [
    for (final entry in headers.entries)
      rust.HeaderField(name: entry.key, value: entry.value),
  ];

  static PinnedException _failed(rust.PinnedFailure? failure) {
    if (failure == null) {
      return const PinnedException(
        rust.PinnedFailureKind.protocol,
        'sin detalle',
      );
    }
    return PinnedException(
      failure.kind,
      failure.detail,
      status: failure.status,
      retryAfter: failure.retryAfter,
    );
  }
}
