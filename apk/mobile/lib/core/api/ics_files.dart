import 'dart:typed_data';

import 'package:file_picker/file_picker.dart';

/// The phone's `.ics` dialogs. Tests inject a fake; the app uses [SystemIcsFiles].
abstract interface class IcsFiles {
  /// The bytes of the `.ics` the person picked, or `null` when they cancel.
  Future<Uint8List?> pick();

  /// Offers to save [bytes] as [name]; `false` when they cancel.
  Future<bool> save(String name, Uint8List bytes);
}

/// The system's picker and save dialog. Both work on bytes in memory.
class SystemIcsFiles implements IcsFiles {
  const SystemIcsFiles();

  @override
  Future<Uint8List?> pick() async {
    final picked = await FilePicker.pickFiles(
      type: FileType.custom,
      allowedExtensions: const ['ics'],
    );
    if (picked.isEmpty) return null;
    return picked.single.readAsBytes();
  }

  @override
  Future<bool> save(String name, Uint8List bytes) async {
    final saved = await FilePicker.saveFile(
      fileName: name,
      bytes: bytes,
      mimeType: 'text/calendar',
    );
    return saved != null;
  }
}
