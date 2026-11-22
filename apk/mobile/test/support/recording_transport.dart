import 'dart:convert';

import 'package:excubia/core/api/chat_transport.dart';

/// Records every call as `METHOD target` with its JSON body. It answers with
/// [answer] (or [bytes] for a binary call), or throws [failure] when one is set.
class RecordingTransport implements ChatTransport {
  final List<String> targets = [];
  final List<Map<String, Object?>> bodies = [];
  final List<String> contentTypes = [];
  String answer = '{"ok":true}';
  List<int> bytes = const [];
  Object? failure;

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    targets.add('$method $target');
    bodies.add(
      body.isEmpty
          ? const {}
          : jsonDecode(utf8.decode(body)) as Map<String, Object?>,
    );
    final error = failure;
    if (error != null) throw error;
    return answer;
  }

  @override
  Future<List<int>> callBytes(String method, String target) async {
    targets.add('$method $target');
    final error = failure;
    if (error != null) throw error;
    return bytes;
  }

  @override
  Future<String> postBytes(
    String target,
    List<int> body,
    String contentType,
  ) async {
    targets.add('POST $target');
    contentTypes.add(contentType);
    bodies.add(jsonDecode(utf8.decode(body)) as Map<String, Object?>);
    final error = failure;
    if (error != null) throw error;
    return answer;
  }
}
