import 'dart:convert';
import 'dart:typed_data';

import 'package:herdr_notify/core/api/chat_transport.dart';
import 'package:herdr_notify/core/api/ics_files.dart';

import 'scripted_transport.dart';

/// One `.ics` sent by the import: its target, its bytes and its content type.
class IcsPost {
  const IcsPost(this.target, this.bytes, this.contentType);

  final String target;
  final List<int> bytes;
  final String contentType;
}

/// The JSON routes of [json] answer as they always do, so failures can be queued on
/// them. The `.ics` routes record what was sent; the export answers the route's text.
class IcsTransport implements ChatTransport {
  IcsTransport(this.json);

  final ScriptedTransport json;
  final List<String> targets = [];
  final List<IcsPost> posts = [];

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) => json.call(method, target, body);

  @override
  Future<List<int>> callBytes(String method, String target) async {
    targets.add('$method $target');
    return utf8.encode(await json.call(method, target));
  }

  @override
  Future<String> postBytes(String target, List<int> body, String contentType) {
    targets.add('POST $target');
    posts.add(IcsPost(target, body, contentType));
    return json.call('POST', target);
  }
}

/// The phone's `.ics` dialogs: [picked] is what the person picks (`null` cancels),
/// and [saveAccepted] whether they save.
class FakeIcsFiles implements IcsFiles {
  FakeIcsFiles({this.picked, this.saveAccepted = true});

  Uint8List? picked;
  bool saveAccepted;
  final List<({String name, Uint8List bytes})> saved = [];

  @override
  Future<Uint8List?> pick() async => picked;

  @override
  Future<bool> save(String name, Uint8List bytes) async {
    saved.add((name: name, bytes: bytes));
    return saveAccepted;
  }
}
