import 'package:image_picker/image_picker.dart';

import '../../core/api/chat_attachment_source.dart';

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
  final name = picked.name;
  return FileAttachmentSource(
    path: picked.path,
    name: name.length > _maxNameLength
        ? name.substring(0, _maxNameLength)
        : name,
    size: await picked.length(),
  );
}
