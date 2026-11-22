import 'dart:typed_data';

import 'package:excubia/core/api/chat_document_files.dart';

/// The phone's open and save dialogs, faked. [outcome] says what the system
/// answers to an open; the last path and mime it was asked to open are kept.
class FakeDocumentFiles implements ChatDocumentFiles {
  OpenOutcome outcome = OpenOutcome.opened;
  bool saveAccepted = true;
  String? openedPath;
  String? openedMime;
  final List<({String name, String mime, Uint8List bytes})> saves = [];

  @override
  Future<OpenOutcome> open(String path, String mime) async {
    openedPath = path;
    openedMime = mime;
    return outcome;
  }

  @override
  Future<bool> save(String name, String mime, Uint8List bytes) async {
    saves.add((name: name, mime: mime, bytes: bytes));
    return saveAccepted;
  }
}
