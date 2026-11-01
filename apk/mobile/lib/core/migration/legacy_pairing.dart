import 'package:flutter/services.dart';

import '../api/pairing_address.dart';

/// The pairing the Kotlin v1 app left behind: where the host was and which
/// device this phone was called. The recovery code is deliberately absent — it
/// is fetched by [LegacyPairingSource.recoveryCode] only when re-enrolment
/// needs it, so it does not sit in this object for the length of the screen.
class LegacyPairing {
  const LegacyPairing({required this.baseUrl, required this.deviceId});

  final String baseUrl;
  final String deviceId;

  /// The host as a person would name it: no scheme, no port, no path.
  String get hostLabel => Uri.tryParse(baseUrl)?.host ?? baseUrl;
}

/// What the migration needs from the old app's storage, so tests do not need
/// a phone. The Android side is `LegacyPairingChannel.kt`.
abstract interface class LegacyPairingSource {
  /// Null when there is nothing usable: no old pairing, or data that cannot be
  /// read or does not describe a host. Corrupt data is ignored, never an error.
  Future<LegacyPairing?> read();

  /// The recovery code, or null when the old app kept none.
  Future<String?> recoveryCode();

  /// Removes the old pairing. Called only after the new one is verified.
  Future<void> clear();
}

class MethodChannelLegacyPairingSource implements LegacyPairingSource {
  MethodChannelLegacyPairingSource({MethodChannel? channel})
    : _channel = channel ?? const MethodChannel(_channelName);

  static const String _channelName = 'dev.herdrnotify/legacy_pairing';

  final MethodChannel _channel;

  @override
  Future<LegacyPairing?> read() async {
    final raw = await _invoke<Object?>('read');
    if (raw is! Map) return null;
    final baseUrl = PairingAddress.normaliseUrl(_text(raw['baseUrl']));
    final deviceId = _text(raw['deviceId']).trim();
    if (baseUrl.isEmpty || deviceId.isEmpty) return null;
    return LegacyPairing(baseUrl: baseUrl, deviceId: deviceId);
  }

  @override
  Future<String?> recoveryCode() async {
    final code = await _invoke<Object?>('recoveryCode');
    final text = _text(code).trim();
    return text.isEmpty ? null : text;
  }

  @override
  Future<void> clear() async {
    await _invoke<void>('clear');
  }

  /// A platform failure reads as "no legacy data": the offer is simply not
  /// shown, and the old data stays where it is.
  Future<T?> _invoke<T>(String method) async {
    try {
      return await _channel.invokeMethod<T>(method);
    } on PlatformException {
      return null;
    } on MissingPluginException {
      return null;
    }
  }

  static String _text(Object? value) => value is String ? value : '';
}
