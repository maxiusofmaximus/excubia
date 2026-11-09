import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:image_picker/image_picker.dart';

import '../../core/api/chat_attachment_source.dart';
import '../../core/api/chat_documents.dart';

/// What the composer's «+» offers.
enum ChatAttachKind { image, file }

/// The longest name the host keeps for a file.
const _maxNameLength = 120;

/// Opens the system gallery for one image. The plugin shrinks it to 2048 px at
/// quality 85; the host re-encodes it to WebP. Null when the user backs out.
Future<ChatAttachmentSource?> pickChatImage() async {
  final picked = await ImagePicker().pickImage(
    source: ImageSource.gallery,
    maxWidth: 2048,
    maxHeight: 2048,
    imageQuality: 85,
  );
  if (picked == null) return null;
  return _sourceOf(picked.path, picked.name, await picked.length());
}

/// Opens the file manager for one document of the chat's types. Null when the
/// user backs out. Android always gives the path of a picked file.
Future<ChatAttachmentSource?> pickChatDocument() async {
  final picked = await FilePicker.pickFiles(
    type: FileType.custom,
    allowedExtensions: chatDocumentExtensions,
  );
  if (picked.isEmpty) return null;
  final file = picked.single;
  final path = file.path;
  if (path == null) return null;
  return _sourceOf(path, file.name, await File(path).length());
}

FileAttachmentSource _sourceOf(String path, String name, int size) =>
    FileAttachmentSource(
      path: path,
      name: name.length > _maxNameLength
          ? name.substring(0, _maxNameLength)
          : name,
      size: size,
    );
