import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../../core/state/chat_attachment_errors.dart';
import '../../core/state/chat_document_opener.dart';
import '../../src/rust/api/chat/attach_model.dart';

/// Opens [file] with the system app. When no app takes it, offers to save it.
/// [onBusy] hears `true` while the download runs and `false` once it ends, so
/// no spinner sits under the save offer.
Future<void> openDocumentFile(
  BuildContext context,
  ChatDocumentOpener documents,
  ChatAttachment file, {
  required void Function(bool busy) onBusy,
}) async {
  onBusy(true);
  DocumentOpen? result;
  try {
    result = await documents.open(file);
  } catch (_) {
    if (context.mounted) _say(context, attachmentOpenFailedText);
  } finally {
    onBusy(false);
  }
  if (result case DocumentNoApp(:final bytes) when context.mounted) {
    await _offerSave(context, documents, file, bytes);
  }
}

/// Downloads the whole file and offers to save it under its name.
Future<void> saveDocumentFile(
  BuildContext context,
  ChatDocumentOpener documents,
  ChatAttachment file,
) async {
  final Uint8List bytes;
  try {
    bytes = await documents.download(file);
  } catch (_) {
    if (context.mounted) _say(context, attachmentSaveFailedText);
    return;
  }
  if (context.mounted) await _save(context, documents, file, bytes);
}

Future<void> _offerSave(
  BuildContext context,
  ChatDocumentOpener documents,
  ChatAttachment file,
  Uint8List bytes,
) => showDialog<void>(
  context: context,
  builder: (dialog) => AlertDialog(
    title: Text(file.name),
    content: const Text(attachmentNoAppText),
    actions: [
      TextButton(
        onPressed: () => Navigator.of(dialog).pop(),
        child: const Text('Cerrar'),
      ),
      FilledButton(
        onPressed: () {
          Navigator.of(dialog).pop();
          _save(context, documents, file, bytes);
        },
        child: const Text(attachmentSaveLabel),
      ),
    ],
  ),
);

Future<void> _save(
  BuildContext context,
  ChatDocumentOpener documents,
  ChatAttachment file,
  Uint8List bytes,
) async {
  try {
    final saved = await documents.save(file, bytes);
    if (saved && context.mounted) _say(context, attachmentSavedText);
  } catch (_) {
    if (context.mounted) _say(context, attachmentSaveFailedText);
  }
}

void _say(BuildContext context, String text) =>
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(text)));
