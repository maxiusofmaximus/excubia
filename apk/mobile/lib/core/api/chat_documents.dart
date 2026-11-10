import '../../src/rust/api/chat/attach_model.dart';

/// The documents the chat takes, by extension. The host keeps the same list.
enum ChatDocKind { sheet, word, slides, pdf, text }

const _kindByExtension = <String, ChatDocKind>{
  'xlsx': ChatDocKind.sheet,
  'csv': ChatDocKind.sheet,
  'docx': ChatDocKind.word,
  'pptx': ChatDocKind.slides,
  'pdf': ChatDocKind.pdf,
  'txt': ChatDocKind.text,
  'md': ChatDocKind.text,
  'json': ChatDocKind.text,
};

/// What the file picker offers and the tray accepts.
final List<String> chatDocumentExtensions = _kindByExtension.keys.toList();

/// The extension of [name] in lower case, or an empty text when it has none.
String extensionOf(String name) {
  final dot = name.lastIndexOf('.');
  return dot < 0 ? '' : name.substring(dot + 1).toLowerCase();
}

/// The kind of a document by its name, or null when the chat does not take it.
ChatDocKind? documentKindOf(String name) => _kindByExtension[extensionOf(name)];

extension ChatDocKindLabel on ChatDocKind {
  String get label => switch (this) {
    ChatDocKind.sheet => 'Hoja de cálculo',
    ChatDocKind.word => 'Documento',
    ChatDocKind.slides => 'Presentación',
    ChatDocKind.pdf => 'PDF',
    ChatDocKind.text => 'Texto',
  };
}

const _previewExtensions = {'xlsx', 'docx', 'pdf'};

/// True for a document the host renders page by page (`previewable` in the
/// contract). The others are text or open with the system app.
bool isPreviewableDocument(String name) =>
    _previewExtensions.contains(extensionOf(name));

const _textExtensions = {'txt', 'md', 'json', 'csv'};

/// True for a document the app reads as plain text, from its own bytes. The
/// host gives these no preview, so the app does not ask it for one.
bool isTextDocument(String name) => _textExtensions.contains(extensionOf(name));

/// True for a sent file that is a document, not a picture. The host does not
/// name the kind on the file yet, so the extension it kept decides.
bool isDocumentFile(ChatAttachment file) => documentKindOf(file.name) != null;

/// A name the phone's file system takes: no separators or control characters,
/// no leading dots, at most 100 characters and the extension kept.
String cacheFileName(String name) {
  final clean = name
      .replaceAll(RegExp(r'[<>:"/\\|?*\x00-\x1f]'), '_')
      .replaceFirst(RegExp(r'^[\s.]+'), '')
      .trim();
  if (clean.isEmpty) return 'documento';
  if (clean.length <= 100) return clean;
  final dot = clean.lastIndexOf('.');
  final ext = dot > 0 && clean.length - dot <= 10 ? clean.substring(dot) : '';
  return clean.substring(0, 100 - ext.length) + ext;
}
