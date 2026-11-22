import 'package:excubia/src/rust/api/pinned.dart' as rust;

/// How a pairing with the short code ended. The Rust types stop here.
sealed class PakePairing {
  const PakePairing();
}

/// The host and the phone proved the same code; [pin] is the key that proof
/// confirmed, the one to store.
class PakePaired extends PakePairing {
  const PakePaired({
    required this.deviceId,
    required this.recoveryCode,
    required this.pin,
    required this.address,
  });

  final String deviceId;
  final String recoveryCode;
  final String pin;
  final String address;
}

/// [message] is in Spanish and never carries the code.
class PakeRefused extends PakePairing {
  const PakeRefused(this.kind, this.message, {this.retryAfter});

  final rust.PakeFailureKind kind;
  final String message;
  final int? retryAfter;

  /// The host could not be shown to be the one that displayed the code: this
  /// is a security warning, never a "try again".
  bool get isSecurity =>
      kind == rust.PakeFailureKind.hostProofInvalid ||
      kind == rust.PakeFailureKind.channelChanged ||
      kind == rust.PakeFailureKind.hostMismatch;
}

PakePairing pakeFromRust(rust.PakePairResult result) {
  final outcome = result.outcome;
  if (outcome != null) {
    return PakePaired(
      deviceId: outcome.deviceId,
      recoveryCode: outcome.recoveryCode,
      pin: outcome.pin,
      address: outcome.address,
    );
  }
  final failure = result.failure;
  if (failure == null) {
    return const PakeRefused(
      rust.PakeFailureKind.protocol,
      'El equipo respondió algo que no entiendo.',
    );
  }
  return PakeRefused(
    failure.kind,
    failure.message,
    retryAfter: failure.retryAfter,
  );
}
