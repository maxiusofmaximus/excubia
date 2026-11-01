import 'dart:convert';

import 'package:herdr_notify/core/api/chat_transport.dart';

/// Records every call as `METHOD target` with its JSON body. It answers with
/// [answer], or throws [failure] when one is set.
class RecordingTransport implements ChatTransport {
  final List<String> targets = [];
  final List<Map<String, Object?>> bodies = [];
  String answer = '{"ok":true}';
  Object? failure;

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    targets.add('$method $target');
    bodies.add(
      body.isEmpty ? const {} : jsonDecode(utf8.decode(body)) as Map<String, Object?>,
    );
    final error = failure;
    if (error != null) throw error;
    return answer;
  }

  @override
  Future<List<int>> callBytes(String method, String target) =>
      throw UnimplementedError();

  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError();
}
