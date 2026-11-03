import 'package:herdr_notify/core/api/pair_discovery.dart';

/// A network with the addresses the test sets. It records the timeout it was
/// asked for and can fail like the platform does.
class FakePairDiscovery implements PairDiscovery {
  FakePairDiscovery({this.found = const [], this.fails = false});

  final List<String> found;
  bool fails;
  final List<Duration> timeouts = [];

  @override
  Future<List<String>> discover({
    Duration timeout = const Duration(seconds: 15),
  }) async {
    timeouts.add(timeout);
    if (fails) throw const PairDiscoveryException();
    return found;
  }
}
