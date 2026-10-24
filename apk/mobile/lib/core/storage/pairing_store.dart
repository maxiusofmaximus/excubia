import 'dart:math';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';

import '../api/pairing_address.dart';
import '../api/pinned_client.dart';

/// Everything a paired phone needs to talk to its gateway.
///
/// Base URL and device id are labels, not secrets. The private key is a secret
/// and lives encrypted by the platform keystore; it never appears on disk in
/// the clear.
class Pairing {
  const Pairing({
    required this.baseUrl,
    required this.deviceId,
    required this.privateKey,
    this.pins,
  });

  final String baseUrl;
  final String deviceId;
  final String privateKey;

  /// The host's pins from its link, or null for a pairing made without one.
  /// Public keys, not secrets: they are stored beside the labels.
  final HostPins? pins;

  static const Pairing empty = Pairing(
    baseUrl: '',
    deviceId: '',
    privateKey: '',
  );

  bool get isComplete =>
      baseUrl.isNotEmpty && deviceId.isNotEmpty && privateKey.isNotEmpty;

  Pairing copyWith({String? baseUrl, String? deviceId, String? privateKey}) =>
      Pairing(
        baseUrl: baseUrl ?? this.baseUrl,
        deviceId: deviceId ?? this.deviceId,
        privateKey: privateKey ?? this.privateKey,
        pins: pins,
      );
}

/// The three operations the pairing store needs, so tests do not need a phone.
abstract interface class KeyValueStore {
  Future<String?> read(String key);

  Future<void> write(String key, String value);

  Future<void> delete(String key);
}

/// `flutter_secure_storage`, which on Android wraps its own key with the
/// hardware keystore and stores the rest AES-GCM encrypted.
class SecureKeyValueStore implements KeyValueStore {
  SecureKeyValueStore({FlutterSecureStorage? storage})
    : _storage = storage ?? const FlutterSecureStorage();

  final FlutterSecureStorage _storage;

  @override
  Future<String?> read(String key) => _storage.read(key: key);

  @override
  Future<void> write(String key, String value) =>
      _storage.write(key: key, value: value);

  @override
  Future<void> delete(String key) => _storage.delete(key: key);
}

class PairingStore {
  PairingStore(this._store);

  static const String baseUrlKey = 'hn_base_url';
  static const String deviceIdKey = 'hn_device_id';
  static const String privateKeyKey = 'hn_private_key';
  static const String recoveryCodeKey = 'hn_recovery_code';
  static const String primaryPinKey = 'hn_pin_primary';
  static const String backupPinKey = 'hn_pin_backup';

  final KeyValueStore _store;

  Future<Pairing> load() async {
    final baseUrl = await _store.read(baseUrlKey) ?? '';
    final deviceId = await _store.read(deviceIdKey) ?? '';
    final privateKey = await _store.read(privateKeyKey) ?? '';
    final primary = await _store.read(primaryPinKey);
    final backup = await _store.read(backupPinKey);
    return Pairing(
      baseUrl: baseUrl,
      deviceId: deviceId,
      privateKey: privateKey,
      pins: primary == null || primary.isEmpty
          ? null
          : HostPins(primary: primary, backup: backup),
    );
  }

  Future<void> save(Pairing pairing) async {
    await _store.write(
      baseUrlKey,
      PairingAddress.normaliseUrl(pairing.baseUrl),
    );
    await _store.write(deviceIdKey, pairing.deviceId.trim());
    await _store.write(privateKeyKey, pairing.privateKey);
    await _savePins(pairing.pins);
  }

  /// A pairing without a link drops any pins kept from an earlier one, so the
  /// stored pins always describe the address that is stored.
  Future<void> _savePins(HostPins? pins) async {
    if (pins == null) {
      await _store.delete(primaryPinKey);
      await _store.delete(backupPinKey);
      return;
    }
    await _store.write(primaryPinKey, pins.primary);
    final backup = pins.backup;
    if (backup == null) {
      await _store.delete(backupPinKey);
    } else {
      await _store.write(backupPinKey, backup);
    }
  }

  /// The recovery code is shown once at pairing; keeping it encrypted lets a
  /// later phase offer it again.
  Future<void> saveRecoveryCode(String code) =>
      _store.write(recoveryCodeKey, code);

  Future<void> clear() async {
    await _store.delete(baseUrlKey);
    await _store.delete(deviceIdKey);
    await _store.delete(privateKeyKey);
    await _store.delete(recoveryCodeKey);
    await _store.delete(primaryPinKey);
    await _store.delete(backupPinKey);
  }

  /// A locally unique id, offered as the default the operator will register.
  static String suggestDeviceId() {
    final random = Random.secure();
    final suffix = List<int>.generate(
      8,
      (_) => random.nextInt(16),
    ).map((value) => value.toRadixString(16)).join();
    return 'hn-$suffix';
  }
}
