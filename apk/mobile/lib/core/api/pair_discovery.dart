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

/// Looks for the PC in up to [attempts] short windows, stopping at the first
/// that finds something: one lost multicast must not end the search. A
/// [PairDiscoveryException] is the phone's own fault and is never retried.
Future<List<String>> discoverWithRetry(
  PairDiscovery discovery, {
  int attempts = 3,
  Duration timeout = const Duration(seconds: 7),
}) async {
  for (var i = 0; i < attempts; i++) {
    final found = await discovery.discover(timeout: timeout);
    if (found.isNotEmpty) return found;
  }
  return const [];
}
