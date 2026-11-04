import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';

/// The host's TLS port (the one in the QR address and in Avanzado).
const hostTlsPort = 47443;

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
/// that finds something: one lost multicast must not end the search. After the
/// first empty window, [fallback] (a TCP sweep) is asked once. A
/// [PairDiscoveryException] is the phone's own fault and is never retried.
Future<List<String>> discoverWithRetry(
  PairDiscovery discovery, {
  int attempts = 3,
  Duration timeout = const Duration(seconds: 7),
  PairDiscovery? fallback,
}) async {
  for (var i = 0; i < attempts; i++) {
    final found = await discovery.discover(timeout: timeout);
    if (found.isNotEmpty) return found;
    if (i == 0 && fallback != null) {
      final swept = await fallback.discover(
        timeout: const Duration(seconds: 4),
      );
      if (swept.isNotEmpty) return swept;
    }
  }
  return const [];
}

/// Finds the PC by connecting to every address of the phone's own /24 networks
/// on [port]. Multicast can fail on some Wi-Fi and a TCP connect does not: the
/// candidates are not trusted, the pin of the QR (or the PAKE tag) is.
class SubnetScanPairDiscovery implements PairDiscovery {
  const SubnetScanPairDiscovery({
    this.port = hostTlsPort,
    this.localAddresses = _localIPv4,
    this.connect = _connect,
    this.concurrency = 48,
  });

  final int port;
  final Future<List<InternetAddress>> Function() localAddresses;
  final Future<bool> Function(InternetAddress ip, int port, Duration timeout)
  connect;
  final int concurrency;

  @override
  Future<List<String>> discover({
    Duration timeout = const Duration(seconds: 15),
  }) async {
    final clock = Stopwatch()..start();
    final found = <String>[];
    try {
      final locals = await localAddresses();
      // The phone's own addresses are never probed.
      final seen = {for (final ip in locals) ip.address};
      final hosts = <InternetAddress>[];
      for (final ip in locals) {
        final prefix = ip.address.substring(0, ip.address.lastIndexOf('.'));
        for (var last = 1; last <= 254; last++) {
          final candidate = '$prefix.$last';
          if (seen.add(candidate)) hosts.add(InternetAddress(candidate));
        }
      }
      var next = 0;
      Future<void> worker() async {
        while (next < hosts.length) {
          final left = timeout - clock.elapsed;
          if (left <= Duration.zero) return;
          final ip = hosts[next++];
          final cap = left < _connectTimeout ? left : _connectTimeout;
          final open = await connect(
            ip,
            port,
            cap,
          ).timeout(cap, onTimeout: () => false);
          if (open) found.add('${ip.address}:$port');
        }
      }

      await Future.wait([for (var i = 0; i < concurrency; i++) worker()]);
    } on Exception {
      // No Wi-Fi or no network permission: there is nothing to scan.
    }
    return List.of(found);
  }
}

const _connectTimeout = Duration(milliseconds: 600);

Future<List<InternetAddress>> _localIPv4() async {
  try {
    final interfaces = await NetworkInterface.list(
      type: InternetAddressType.IPv4,
    );
    return [
      for (final interface in interfaces)
        for (final ip in interface.addresses)
          if (!ip.isLoopback && !ip.isLinkLocal) ip,
    ];
  } on Exception {
    return const [];
  }
}

Future<bool> _connect(InternetAddress ip, int port, Duration timeout) async {
  try {
    (await Socket.connect(ip, port, timeout: timeout)).destroy();
    return true;
  } on Exception {
    return false;
  }
}
