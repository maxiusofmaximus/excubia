import 'package:herdr_notify/core/api/pinned_client.dart';

/// Stands in for the Rust transport: it records what would have gone out and
/// answers with a canned reply or failure, so no socket is opened.
class FakePinnedClient implements PinnedClient {
  FakePinnedClient({this.answer, this.failure});

  final PinnedAnswer? answer;
  final PinnedException? failure;
  final List<String> targets = [];
  final List<String> addresses = [];
  final List<Map<String, String>> headers = [];

  @override
  Future<PinnedAnswer> request({
    required List<String> addresses,
    required HostPins pins,
    required String method,
    required String target,
    required Map<String, String> headers,
    required List<int> body,
  }) async {
    this.addresses.addAll(addresses);
    targets.add(target);
    this.headers.add(headers);
    final failure = this.failure;
    if (failure != null) throw failure;
    return answer!;
  }

  @override
  Stream<String> eventLines({
    required List<String> addresses,
    required HostPins pins,
    required String target,
    required Map<String, String> headers,
  }) => const Stream.empty();
}
