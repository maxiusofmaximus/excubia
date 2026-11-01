import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/chat/attach_model.dart';

import '../../core/state/chat_attachment_images.dart';
import '../theme.dart';

/// The thumbnails of a message's files. A file still loading shows a spinner,
/// one that failed a broken-image mark; a tap opens it full size.
class ChatAttachmentStrip extends StatelessWidget {
  const ChatAttachmentStrip({
    super.key,
    required this.attachments,
    required this.images,
  });

  final List<ChatAttachment> attachments;
  final ChatAttachmentImages images;

  @override
  Widget build(BuildContext context) {
    return Wrap(
      spacing: Spacing.xs,
      runSpacing: Spacing.xs,
      children: [
        for (final file in attachments) _Thumb(file: file, images: images),
      ],
    );
  }
}

class _Thumb extends StatelessWidget {
  const _Thumb({required this.file, required this.images});

  final ChatAttachment file;
  final ChatAttachmentImages images;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return FutureBuilder<Uint8List>(
      future: images.load(file.id),
      builder: (context, snapshot) {
        final bytes = snapshot.data;
        return Semantics(
          label: bytes == null ? '${file.name}, no disponible' : file.name,
          image: true,
          button: bytes != null,
          child: GestureDetector(
            onTap: bytes == null ? null : () => _open(context, bytes),
            child: Container(
              width: 96,
              height: 96,
              clipBehavior: Clip.antiAlias,
              decoration: BoxDecoration(
                color: c.raised,
                borderRadius: BorderRadius.circular(10),
              ),
              child: bytes == null
                  ? Center(
                      child: snapshot.hasError
                          ? Icon(
                              Icons.broken_image_outlined,
                              color: c.textMuted,
                            )
                          : const SizedBox(
                              width: 20,
                              height: 20,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            ),
                    )
                  : Image.memory(
                      bytes,
                      fit: BoxFit.cover,
                      gaplessPlayback: true,
                      semanticLabel: file.name,
                      errorBuilder: (_, _, _) => Center(
                        child: Icon(
                          Icons.broken_image_outlined,
                          color: c.textMuted,
                        ),
                      ),
                    ),
            ),
          ),
        );
      },
    );
  }

  void _open(BuildContext context, Uint8List bytes) {
    Navigator.of(context).push<void>(
      MaterialPageRoute<void>(
        builder: (_) => ChatAttachmentViewer(name: file.name, bytes: bytes),
      ),
    );
  }
}

/// One image full size, pinch to zoom. Back closes it, as on any route.
class ChatAttachmentViewer extends StatelessWidget {
  const ChatAttachmentViewer({
    super.key,
    required this.name,
    required this.bytes,
  });

  final String name;
  final Uint8List bytes;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: Colors.black,
      appBar: AppBar(
        backgroundColor: Colors.black,
        foregroundColor: Colors.white,
        title: Text(name, overflow: TextOverflow.ellipsis),
      ),
      body: Center(
        child: InteractiveViewer(
          maxScale: 5,
          child: Image.memory(bytes, fit: BoxFit.contain, semanticLabel: name),
        ),
      ),
    );
  }
}
