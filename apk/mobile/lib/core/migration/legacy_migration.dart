import '../../src/rust/api/signing.dart';
import '../api/host_client.dart';
import '../api/pairing_api.dart';
import '../storage/pairing_store.dart';
import 'legacy_pairing.dart';

/// How re-enrolment with the old app's recovery code ended.
enum LegacyOutcome {
  /// The host took this phone's new key, and a signed call with it answered.
  migrated,

  /// The host does not know that recovery code: the device was revoked, or
  /// it was rotated since. The old data is kept.
  rejected,

  /// The host could not be reached (off, not on the tailnet, timed out).
  unreachable,

  /// The host took the new key but the signed check failed. Nothing was saved
  /// on this phone and the old data is kept, though the host's recovery code
  /// has already rotated.
  unverified,

  /// The old app kept no recovery code, so there is nothing to re-enrol with.
  noRecoveryCode,
}

class LegacyAdoption {
  const LegacyAdoption(this.outcome, {this.pairing});

  final LegacyOutcome outcome;

  /// The pairing this phone now uses, set only when [outcome] is migrated.
  final Pairing? pairing;
}

/// Offers the old app's pairing and, when the user accepts, moves it to a new
/// key of this app.
///
/// The old data is removed only after the new pairing answers a signed call.
/// Every other path leaves it where it was.
class LegacyMigration {
  LegacyMigration({
    required this.source,
    required this.values,
    required this.store,
    required this.api,
    required this.newKey,
    required this.verify,
  });

  /// The production wiring: the Kotlin channel, the real pairing API and key
  /// generator, and a check that makes one signed `GET /v1/state`.
  factory LegacyMigration.standard({
    required PairingStore store,
    required KeyValueStore values,
  }) => LegacyMigration(
    source: MethodChannelLegacyPairingSource(),
    values: values,
    store: store,
    api: PairingApi(),
    newKey: generateDeviceKey,
    verify: (pairing) => HostClient(
      baseUrl: pairing.baseUrl,
      deviceId: pairing.deviceId,
      privateKeyBase64: pairing.privateKey,
    ).state(),
  );

  static const String _declinedKey = 'hn_legacy_offer_declined';

  final LegacyPairingSource source;
  final KeyValueStore values;
  final PairingStore store;
  final PairingApi api;
  final DeviceKeyMaterial Function() newKey;

  /// Throws when the candidate pairing cannot answer a signed call.
  final Future<void> Function(Pairing candidate) verify;

  /// The old pairing to offer, or null. Call only when this phone has no
  /// pairing of its own.
  Future<LegacyPairing?> offer() async {
    if (await values.read(_declinedKey) == '1') return null;
    return source.read();
  }

  /// The user chose to pair from scratch. The offer is not shown again.
  Future<void> decline() => values.write(_declinedKey, '1');

  Future<LegacyAdoption> adopt(LegacyPairing legacy) async {
    final code = await source.recoveryCode();
    if (code == null) {
      return const LegacyAdoption(LegacyOutcome.noRecoveryCode);
    }
    final material = newKey();
    final PairResponse response;
    try {
      response = await api.pair(
        legacy.baseUrl,
        PairRequest(
          publicKey: material.publicKeyB64,
          label: legacy.deviceId,
          code: code,
        ),
      );
    } on PairingException catch (e) {
      return LegacyAdoption(
        e.status == 403 ? LegacyOutcome.rejected : LegacyOutcome.unreachable,
      );
    }
    final candidate = Pairing(
      baseUrl: legacy.baseUrl,
      deviceId: response.deviceId,
      privateKey: material.privateKeyB64,
    );
    try {
      await verify(candidate);
    } on Exception {
      return const LegacyAdoption(LegacyOutcome.unverified);
    }
    await store.save(candidate);
    final rotated = response.recoveryCode;
    if (rotated != null && rotated.isNotEmpty) {
      await store.saveRecoveryCode(rotated);
    }
    await source.clear();
    return LegacyAdoption(LegacyOutcome.migrated, pairing: candidate);
  }
}
