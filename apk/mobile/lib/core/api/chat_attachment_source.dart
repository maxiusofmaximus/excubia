import 'dart:io';

import '../../src/rust/api/chat/attach_upload.dart';
import 'chat_documents.dart';

/// Most files one message may carry.
const maxChatAttachments = 10;

/// Largest file the host accepts, in bytes (10 MiB).
const maxAttachmentBytes = 10 * 1024 * 1024;

/// The picture types the picker may hand over; the signature decides the rest.
const _imageExtensions = {'png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp', 'heic', 'heif'};

/// One picked file, read in pieces so a 10 MiB file is never held twice.
abstract interface class ChatAttachmentSource {
  String get name;
  int get size;

  /// The bytes from [offset], at most [length] of them.
  Future<List<int>> read(int offset, int length);

  /// SHA-256 of the whole file in hex; null when it cannot be read.
  Future<String?> sha256();
}

/// A picked file on disk. Each read opens the file, so no handle outlives a chunk.
class FileAttachmentSource implements ChatAttachmentSource {
  FileAttachmentSource({
    required this.path,
    required this.name,
    required this.size,
  });

  final String path;
  @override
  final String name;
  @override
  final int size;

  @override
  Future<List<int>> read(int offset, int length) async {
    final file = await File(path).open();
    try {
      await file.setPosition(offset);
      return await file.read(length);
    } finally {
      await file.close();
    }
  }

  @override
  Future<String?> sha256() => attachmentSha256(path: path);
}

/// True when the first bytes are PNG, JPEG or WebP magic. The file name and the
/// picker's type are not trusted.
bool isSupportedImage(List<int> head) =>
    _startsAt(head, 0, const [0x89, 0x50, 0x4E, 0x47]) ||
    _startsAt(head, 0, const [0xFF, 0xD8, 0xFF]) ||
    (_startsAt(head, 0, 'RIFF'.codeUnits) &&
        _startsAt(head, 8, 'WEBP'.codeUnits));

/// The contract code that refuses [source] before any byte goes up, or null
/// when it may be uploaded. A document is judged by its name and size only;
/// the host reads its signature.
Future<String?> refusalOf(ChatAttachmentSource source) async {
  if (documentKindOf(source.name) != null) {
    return source.size > maxAttachmentBytes ? 'document_too_large' : null;
  }
  final extension = extensionOf(source.name);
  if (extension.isNotEmpty && !_imageExtensions.contains(extension)) {
    return 'unsupported_type';
  }
  if (source.size > maxAttachmentBytes) return 'image_too_large';
  final head = await source.read(0, 12);
  return isSupportedImage(head) ? null : 'unsupported_image';
}

bool _startsAt(List<int> bytes, int offset, List<int> magic) {
  if (bytes.length < offset + magic.length) return false;
  for (var i = 0; i < magic.length; i++) {
    if (bytes[offset + i] != magic[i]) return false;
  }
  return true;
}
