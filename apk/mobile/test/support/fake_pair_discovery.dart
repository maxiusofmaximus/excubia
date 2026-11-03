import 'package:herdr_notify/core/api/pair_discovery.dart';

/// A network with the addresses the test sets. It records the timeout it was
/// asked for and can fail like the platform does. With [script], call n
/// answers `script[n]` (the last one repeats); otherwise it answers [found].
class FakePairDiscovery implements PairDiscovery {
  FakePairDiscovery({
    this.found = const [],
    this.fails = false,
    this.script = const [],
  });

  final List<String> found;
  final List<List<String>> script;
  bool fails;
  final List<Duration> timeouts = [];

  @override
  Future<List<String>> discover({
    Duration timeout = const Duration(seconds: 15),
  }) async {
    timeouts.add(timeout);
    if (fails) throw const PairDiscoveryException();
    if (script.isEmpty) return found;
    return script[(timeouts.length - 1).clamp(0, script.length - 1)];
  }
}
