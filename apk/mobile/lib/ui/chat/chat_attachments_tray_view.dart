import 'dart:io';

import 'package:flutter/material.dart';

import '../../core/api/chat_attachment_source.dart';
import '../../core/api/chat_documents.dart';
import '../../core/state/chat_attachment_tray.dart';
import '../mail/mail_facts.dart';
import '../theme.dart';
import 'chat_document_card.dart';

/// The files the composer holds: a thumbnail or a document icon, the name, the
/// size, the upload's progress or refusal, and «Quitar».
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
    final kind = documentKindOf(draft.name);
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
              _Preview(path: path, kind: kind),
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
                      sizeLabel(draft.source.size),
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
  const _Preview({required this.path, required this.kind});

  final String? path;

  /// Set for a document, which shows its icon instead of a picture.
  final ChatDocKind? kind;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final path = this.path;
    final kind = this.kind;
    return ClipRRect(
      borderRadius: BorderRadius.circular(8),
      child: SizedBox(
        width: 48,
        height: 48,
        child: kind != null
            ? Icon(documentIcon(kind), color: c.textMuted)
            : path == null
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
