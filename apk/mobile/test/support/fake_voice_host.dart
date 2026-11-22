import 'package:excubia/core/api/host_client.dart';

import 'fake_chat_transport.dart';

/// The host's voice route: answers with a canned transcript or refuses, and
/// keeps every request that reached it.
class FakeVoiceHost extends FakeChatTransport {
  FakeVoiceHost({this.answer = '{"text":"hola mundo","ms":1}', this.refusal})
    : super({});

  final String answer;
  final HostError? refusal;
  final List<({String target, List<int> body, String contentType})> posts = [];

  @override
  Future<String> postBytes(
    String target,
    List<int> body,
    String contentType,
  ) async {
    posts.add((target: target, body: body, contentType: contentType));
    final error = refusal;
    if (error != null) throw error;
    return answer;
  }
}
