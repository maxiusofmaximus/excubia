import 'package:flutter/services.dart';

/// The phone could not look for the PC on the network.
class PairDiscoveryException implements Exception {
  const PairDiscoveryException();

  String get message => 'No pude buscar el PC en la red. Comprueba la Wi-Fi.';

  @override
  String toString() => message;
}

/// Finds the PC that is showing a pairing code. Tests put a fake in its place.
abstract interface class PairDiscovery {
  /// The `ip:port` of every PC found; empty when none answered in [timeout].
  Future<List<String>> discover({
    Duration timeout = const Duration(seconds: 15),
  });
}

/// mDNS through `dev.herdrnotify/discovery` (HerdrDiscoveryPlugin.kt).
class MethodChannelPairDiscovery implements PairDiscovery {
  const MethodChannelPairDiscovery();

  static const MethodChannel _channel = MethodChannel(
    'dev.herdrnotify/discovery',
  );

  @override
  Future<List<String>> discover({
    Duration timeout = const Duration(seconds: 15),
  }) async {
    try {
      final found = await _channel.invokeListMethod<String>('discover', {
        'timeoutMs': timeout.inMilliseconds,
      });
      return found ?? const [];
    } on PlatformException {
      throw const PairDiscoveryException();
    }
  }
}
