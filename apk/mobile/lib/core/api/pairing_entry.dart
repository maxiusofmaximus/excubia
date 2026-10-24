import 'package:herdr_notify/src/rust/api/pinned.dart' as rust;

import 'pair_qr.dart';

/// What the pairing screen can do with a scanned or pasted string.
sealed class PairingEntry {
  const PairingEntry();
}

/// The CORE11 link: where the host is, its code, and the keys it must present.
final class PairingEntryPinned extends PairingEntry {
  const PairingEntryPinned(this.link);

  final rust.PairingLinkInfo link;
}

/// The earlier QR without a pin. It is still read, but it carries no
/// verification, so the screen treats its connection as plain.
final class PairingEntryPlain extends PairingEntry {
  const PairingEntryPlain(this.payload);

  final PairingQrPayload payload;
}

final class PairingEntryInvalid extends PairingEntry {
  const PairingEntryInvalid(this.reason);

  final String reason;
}

/// A link with `h=` is the pinned format and is parsed by the Rust core, which
/// owns the version, expiry and pin rules. Anything else takes the old path.
PairingEntry parsePairingEntry(String raw) {
  final text = raw.trim();
  if (_isPinnedLink(text)) {
    final result = rust.parsePairingLink(uri: text);
    final link = result.link;
    if (link != null) return PairingEntryPinned(link);
    return PairingEntryInvalid(_linkReason(result.failure));
  }
  final legacy = PairingQrPayload.parse(text);
  return switch (legacy) {
    PairingQrValid(:final payload) => PairingEntryPlain(payload),
    PairingQrInvalid(:final reason) => PairingEntryInvalid(reason),
  };
}

bool _isPinnedLink(String text) =>
    text.startsWith('${PairingQrPayload.scheme}://pair?') &&
    (Uri.tryParse(text)?.queryParameters.containsKey('h') ?? false);

String _linkReason(rust.PinnedFailure? failure) => switch (failure?.kind) {
  rust.PinnedFailureKind.linkExpired =>
    'El código ha caducado. Genera uno nuevo en el equipo.',
  rust.PinnedFailureKind.unsupportedLinkVersion =>
    'Este código QR es de una versión más nueva; actualiza la app.',
  _ => 'El enlace de emparejamiento no se pudo leer.',
};
