import 'dart:async';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/pair_discovery.dart';

const _port = 47443;

/// A phone whose [locals] are the given addresses. Probes answer [open] unless
/// [connect] is given; every probed address is recorded in [probed].
SubnetScanPairDiscovery _scan({
  List<String> locals = const ['192.168.1.10'],
  Set<String> open = const {},
  List<String>? probed,
  Future<bool> Function(InternetAddress ip, int port, Duration timeout)?
  connect,
}) {
  return SubnetScanPairDiscovery(
    localAddresses: () async => [for (final ip in locals) InternetAddress(ip)],
    connect:
        connect ??
        (ip, port, timeout) async {
          probed?.add(ip.address);
          return open.contains(ip.address);
        },
  );
}

Future<List<InternetAddress>> _noWifi() async =>
    throw const SocketException('no wifi');

void main() {
  group('SubnetScanPairDiscovery', () {
    test(
      'finds the open hosts of the own /24, never probing the phone itself',
      () async {
        final probed = <String>[];
        final found = await _scan(
          open: {'192.168.1.20', '192.168.1.77'},
          probed: probed,
        ).discover();

        expect(
          found,
          unorderedEquals(['192.168.1.20:$_port', '192.168.1.77:$_port']),
        );
        expect(probed, hasLength(253));
        expect(probed, isNot(contains('192.168.1.10')));
        expect(probed, isNot(contains('192.168.1.0')));
        expect(probed, isNot(contains('192.168.1.255')));
      },
    );

    test('two networks are both swept, each without its own address', () async {
      final probed = <String>[];
      await _scan(
        locals: const ['192.168.1.10', '10.0.0.5'],
        probed: probed,
      ).discover();

      expect(probed, hasLength(506));
      expect(probed.toSet(), hasLength(506));
    });

    test('no open host is an empty list', () async {
      expect(await _scan().discover(), isEmpty);
    });

    test('no Wi-Fi scans nothing and is an empty list', () async {
      final probed = <String>[];
      final found = await _scan(locals: const [], probed: probed).discover();

      expect(found, isEmpty);
      expect(probed, isEmpty);
    });

    test('a failing interface query is an empty list', () async {
      const failing = SubnetScanPairDiscovery(localAddresses: _noWifi);

      expect(await failing.discover(), isEmpty);
    });

    test('a silent network is cut at the timeout, whole', () async {
      final clock = Stopwatch()..start();
      final found = await _scan(
        connect: (ip, port, timeout) => Completer<bool>().future,
      ).discover(timeout: const Duration(milliseconds: 300));

      expect(found, isEmpty);
      expect(clock.elapsed, lessThan(const Duration(seconds: 2)));
    });

    test(
      'a slow scan stops at the timeout instead of probing every host',
      () async {
        final probed = <String>[];
        await _scan(
          connect: (ip, port, timeout) async {
            probed.add(ip.address);
            await Future<void>.delayed(const Duration(milliseconds: 100));
            return false;
          },
        ).discover(timeout: const Duration(milliseconds: 300));

        expect(probed.length, lessThan(253));
      },
    );
  });
}
