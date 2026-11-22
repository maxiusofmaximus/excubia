import 'dart:convert';

import 'package:excubia/core/api/chat_transport.dart';

/// A 1×1 PNG, so the screen can decode what the tests show.
final List<int> kPreviewPng = base64Decode(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=',
);

/// Answers the preview route by its `page` query: a PNG unless the test gives
/// the page other bytes or an error. Every call is kept in [calls].
class FakePreviewTransport implements ChatTransport {
  final List<String> calls = <String>[];
  final Map<int, List<int>> bytesByPage = <int, List<int>>{};
  final Map<int, Object> errorByPage = <int, Object>{};

  @override
  Future<List<int>> callBytes(String method, String target) async {
    calls.add('$method $target');
    final page = int.parse(Uri.parse(target).queryParameters['page'] ?? '1');
    final error = errorByPage[page];
    if (error != null) throw error;
    return bytesByPage[page] ?? kPreviewPng;
  }

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) => throw UnimplementedError('la vista previa solo pide bytes');

  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError('la vista previa solo pide bytes');
}
