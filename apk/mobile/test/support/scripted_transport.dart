import 'dart:convert';

import 'package:herdr_notify/core/api/chat_transport.dart';

/// Answers by `METHOD path` (the query is ignored) and can fail the next calls
/// of one route, so a test refuses a decision and then reads its state back.
class ScriptedTransport implements ChatTransport {
  ScriptedTransport(this.routes);

  final Map<String, String> routes;
  final Map<String, List<Object>> _errors = <String, List<Object>>{};
  final List<String> calls = <String>[];
  final Map<String, List<Map<String, Object?>>> bodies =
      <String, List<Map<String, Object?>>>{};

  void failNext(String key, Object error) =>
      _errors.putIfAbsent(key, () => <Object>[]).add(error);

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    final path = target.split('?').first;
    final key = '$method $path';
    calls.add(key);
    if (body.isNotEmpty) {
      final sent = jsonDecode(utf8.decode(body)) as Map<String, Object?>;
      bodies.putIfAbsent(key, () => []).add(sent);
    }
    final queued = _errors[key];
    if (queued != null && queued.isNotEmpty) throw queued.removeAt(0);
    final response = routes[key] ?? routes[path];
    if (response == null) throw StateError('no canned response for $key');
    return response;
  }
}
