import 'dart:io';
import 'dart:typed_data';

import 'package:path_provider/path_provider.dart';

import '../../src/rust/api/chat/attach_model.dart';
import '../api/chat_document_files.dart';
import '../api/chat_document_previews.dart';
import '../api/chat_documents.dart';

/// Opened documents stay this long in the cache; the next start drops them.
const _keepFor = Duration(hours: 24);

/// What opening a document came to: an app took it, or no app opens its type
/// and the bytes are kept so the person can save them.
sealed class DocumentOpen {
  const DocumentOpen();
}

final class DocumentOpened extends DocumentOpen {
  const DocumentOpened();
}

final class DocumentNoApp extends DocumentOpen {
  const DocumentNoApp(this.bytes);

  final Uint8List bytes;
}

/// The app's cache folder where opened documents are written.
Future<Directory> chatDocumentsDirectory() async {
  final temp = await getTemporaryDirectory();
  return Directory('${temp.path}/chat_documents');
}

/// The documents of the timeline: each is downloaded, then opened with the
/// system app for its type, saved, or shown page by page through [previews].
class ChatDocumentOpener {
  ChatDocumentOpener({
    required this.fetch,
    required this.files,
    required this.root,
    required this.previews,
  });

  final Future<List<int>> Function(String attachmentId) fetch;
  final ChatDocumentFiles files;
  final Future<Directory> Function() root;
  final ChatDocumentPreviews previews;

  /// The whole file, as the host stores it.
  Future<Uint8List> download(ChatAttachment file) async =>
      Uint8List.fromList(await fetch(file.id));

  /// Writes the file into the cache under its own name and opens it.
  Future<DocumentOpen> open(ChatAttachment file) async {
    final bytes = await download(file);
    final folder = await root();
    await folder.create(recursive: true);
    final path = '${folder.path}/${cacheFileName(file.name)}';
    await File(path).writeAsBytes(bytes, flush: true);
    final outcome = await files.open(path, file.mime);
    return outcome == OpenOutcome.opened
        ? const DocumentOpened()
        : DocumentNoApp(bytes);
  }

  Future<bool> save(ChatAttachment file, Uint8List bytes) =>
      files.save(file.name, file.mime, bytes);
}

/// Drops the files in [root] last changed more than a day before [now]. A file
/// another app still holds is kept; the next start tries again.
Future<void> pruneOpenedDocuments(Directory root, {required DateTime now}) async {
  if (!await root.exists()) return;
  await for (final entry in root.list()) {
    if (entry is! File) continue;
    try {
      final age = now.difference((await entry.stat()).modified);
      if (age > _keepFor) await entry.delete();
    } on FileSystemException {
      // Still open elsewhere, or already gone.
    }
  }
}

/// Startup hook: cleans the cache of opened documents. It never blocks start.
Future<void> pruneChatDocuments() async {
  try {
    await pruneOpenedDocuments(await chatDocumentsDirectory(), now: DateTime.now());
  } on FileSystemException {
    // No cache folder yet, or it cannot be listed: nothing to clean.
  }
}
