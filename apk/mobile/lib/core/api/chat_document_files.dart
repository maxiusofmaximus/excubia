import 'dart:typed_data';

import 'package:file_picker/file_picker.dart';
import 'package:open_filex/open_filex.dart';

/// What the system did with a file it was asked to open.
enum OpenOutcome { opened, noApp }

/// The phone's open and save dialogs for a document. Tests inject a fake; the
/// app uses [SystemChatDocumentFiles].
abstract interface class ChatDocumentFiles {
  /// Opens the file at [path] with the app the system picks for [mime]. Throws
  /// for any failure other than a missing app.
  Future<OpenOutcome> open(String path, String mime);

  /// Offers to save [bytes] as [name]; `false` when the person cancels.
  Future<bool> save(String name, String mime, Uint8List bytes);
}

/// The system's open and save dialogs. The plugin's own FileProvider covers the
/// app cache, where the documents are written.
class SystemChatDocumentFiles implements ChatDocumentFiles {
  const SystemChatDocumentFiles();

  @override
  Future<OpenOutcome> open(String path, String mime) async {
    final result = await OpenFilex.open(path, type: mime);
    return switch (result.type) {
      ResultType.done => OpenOutcome.opened,
      ResultType.noAppToOpen => OpenOutcome.noApp,
      _ => throw StateError(result.message),
    };
  }

  @override
  Future<bool> save(String name, String mime, Uint8List bytes) async {
    final saved = await FilePicker.saveFile(
      fileName: name,
      bytes: bytes,
      mimeType: mime,
    );
    return saved != null;
  }
}
