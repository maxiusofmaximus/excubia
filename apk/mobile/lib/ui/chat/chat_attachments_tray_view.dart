import 'dart:io';

import 'package:flutter/material.dart';

import '../../core/api/chat_attachment_source.dart';
import '../../core/state/chat_attachment_tray.dart';
import '../theme.dart';

/// The images the composer holds: a thumbnail, the name, the size, the upload's
/// progress or refusal, and «Quitar».
class ChatAttachmentTrayView extends StatelessWidget {
  const ChatAttachmentTrayView({super.key, required this.tray});

  final ChatAttachmentTray tray;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 132,
      child: ListView.separated(
        scrollDirection: Axis.horizontal,
        itemCount: tray.items.length,
        separatorBuilder: (_, _) => const SizedBox(width: Spacing.sm),
        itemBuilder: (context, index) {
          final draft = tray.items[index];
          return _DraftTile(draft: draft, onRemove: () => tray.remove(draft));
        },
      ),
    );
  }
}

class _DraftTile extends StatelessWidget {
  const _DraftTile({required this.draft, required this.onRemove});

  final AttachmentDraft draft;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final source = draft.source;
    final path = source is FileAttachmentSource ? source.path : null;
    final failed = draft.status == DraftStatus.failed;
    final percent = (draft.progress * 100).round();
    return Container(
      width: 164,
      padding: const EdgeInsets.all(Spacing.sm),
      decoration: BoxDecoration(
        color: c.raised,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: failed ? c.statusBlocked : c.outlineSoft),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Row(
            children: [
              _Preview(path: path),
              const SizedBox(width: Spacing.sm),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      draft.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: context.text.small,
                    ),
                    Text(
                      _sizeLabel(draft.source.size),
                      style: context.text.label.copyWith(color: c.textMuted),
                    ),
                  ],
                ),
              ),
            ],
          ),
          if (draft.status == DraftStatus.uploading ||
              draft.status == DraftStatus.queued)
            Semantics(
              liveRegion: true,
              label: 'Subiendo ${draft.name}',
              value: '$percent %',
              child: LinearProgressIndicator(
                value: draft.status == DraftStatus.uploading
                    ? draft.progress
                    : null,
              ),
            ),
          if (draft.error case final error?)
            Semantics(
              liveRegion: true,
              child: Text(
                error,
                style: context.text.small.copyWith(color: c.statusBlocked),
              ),
            ),
          Align(
            alignment: Alignment.centerRight,
            child: TextButton(
              onPressed: onRemove,
              style: TextButton.styleFrom(minimumSize: const Size(48, 48)),
              child: const Text('Quitar'),
            ),
          ),
        ],
      ),
    );
  }
}

class _Preview extends StatelessWidget {
  const _Preview({required this.path});

  final String? path;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final path = this.path;
    return ClipRRect(
      borderRadius: BorderRadius.circular(8),
      child: SizedBox(
        width: 48,
        height: 48,
        child: path == null
            ? Icon(Icons.image_outlined, color: c.textMuted)
            : Image.file(
                File(path),
                fit: BoxFit.cover,
                cacheWidth: 96,
                errorBuilder: (_, _, _) =>
                    Icon(Icons.broken_image_outlined, color: c.textMuted),
              ),
      ),
    );
  }
}

String _sizeLabel(int bytes) => bytes < 1024 * 1024
    ? '${(bytes / 1024).ceil()} KB'
    : '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
