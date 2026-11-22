import 'package:excubia/src/rust/api/pake_link.dart' as rust;
import 'package:excubia/src/rust/api/pinned.dart' as rust;

import '../app_info.dart';

/// What the pairing screen can do with a scanned or pasted string.
sealed class PairingEntry {
  const PairingEntry();
}

/// The CORE11 link: where the host is, its code, and the keys it must present.
final class PairingEntryPinned extends PairingEntry {
  const PairingEntryPinned(this.link);

  final rust.PairingLinkInfo link;
}

/// The v2 QR: the short code and the fingerprint of the PC that shows it, no
/// address. The phone finds the PC on the network and checks its key against
/// [hostFingerprint] before sending anything.
final class PairingEntryPake extends PairingEntry {
  const PairingEntryPake({
    required this.code,
    required this.hostFingerprint,
    required this.expiresAt,
  });

  final String code;
  final String hostFingerprint;
  final DateTime expiresAt;

  @override
  String toString() => 'PairingEntryPake(expiresAt: $expiresAt)';
}

final class PairingEntryInvalid extends PairingEntry {
  const PairingEntryInvalid(this.reason);

  final String reason;
}

/// A `v=2` link is the PAKE QR; a link with `h=` is the pinned format. Both are
/// parsed by the Rust core, which owns the version and pin rules; the v2 expiry
/// is judged here with [clock], so a test can move time. Anything else is
/// refused, the old unpinned `u=` QR included.
PairingEntry parsePairingEntry(String raw, {DateTime Function()? clock}) {
  final text = raw.trim();
  if (_isPakeLink(text)) return _parsePake(text, (clock ?? DateTime.now)());
  if (_isPinnedLink(text)) {
    final result = rust.parsePairingLink(uri: text);
    final link = result.link;
    if (link != null) return PairingEntryPinned(link);
    return PairingEntryInvalid(_linkReason(result.failure));
  }
  return PairingEntryInvalid(
    'El código no es de $appName o ya no se admite. Genera uno nuevo en el PC.',
  );
}

PairingEntry _parsePake(String text, DateTime now) {
  final result = rust.parsePakeLink(uri: text);
  final link = result.link;
  if (link == null) return PairingEntryInvalid(_pakeReason(result.failure));
  final expiresAt = DateTime.fromMillisecondsSinceEpoch(
    link.expiresAt * 1000,
    isUtc: true,
  );
  if (!expiresAt.isAfter(now)) {
    return const PairingEntryInvalid('El QR caducó. Genera otro en el PC.');
  }
  return PairingEntryPake(
    code: link.code,
    hostFingerprint: link.host,
    expiresAt: expiresAt,
  );
}

String _pakeReason(rust.PinnedFailure? failure) =>
    failure?.kind == rust.PinnedFailureKind.unsupportedLinkVersion
    ? 'Este código QR es de una versión más nueva; actualiza la app.'
    : 'El QR no es válido. Genera otro en el PC.';

bool _isPairLink(String text) => text.startsWith('excubia://pair?');

bool _isPakeLink(String text) =>
    _isPairLink(text) && Uri.tryParse(text)?.queryParameters['v'] == '2';

bool _isPinnedLink(String text) =>
    _isPairLink(text) &&
    (Uri.tryParse(text)?.queryParameters.containsKey('h') ?? false);

String _linkReason(rust.PinnedFailure? failure) => switch (failure?.kind) {
  rust.PinnedFailureKind.linkExpired =>
    'El código ha caducado. Genera uno nuevo en el equipo.',
  rust.PinnedFailureKind.unsupportedLinkVersion =>
    'Este código QR es de una versión más nueva; actualiza la app.',
  _ => 'El enlace de emparejamiento no se pudo leer.',
};
