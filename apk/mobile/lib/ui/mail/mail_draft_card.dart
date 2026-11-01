import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';

import '../theme.dart';
import '../widgets/surface_card.dart';

/// The «Revisar borrador» row of the timeline: what Alfred wrote, to whom, and
/// the button that opens the sheet. The operator reads it there and decides.
class DraftReviewCard extends StatelessWidget {
  const DraftReviewCard({
    super.key,
    required this.draft,
    required this.preview,
    required this.onOpen,
  });

  final MailDraftCard draft;
  final String preview;

  /// Null leaves the button disabled, as in a timeline with no sheet to open.
  final VoidCallback? onOpen;

  @override
  Widget build(BuildContext context) {
    final text = context.text;
    return SurfaceCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Alfred preparó un borrador', style: text.label),
          const SizedBox(height: Spacing.xs),
          Text(
            draft.subject.isEmpty ? 'Sin asunto' : draft.subject,
            style: text.title,
          ),
          const SizedBox(height: Spacing.xs),
          Text('Para: ${draft.to.join(', ')}', style: text.bodySecondary),
          if (preview.isNotEmpty) ...[
            const SizedBox(height: Spacing.xs),
            Text(
              preview,
              style: text.body,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
            ),
          ],
          const SizedBox(height: Spacing.md),
          OutlinedButton(
            style: OutlinedButton.styleFrom(minimumSize: const Size(0, 44)),
            onPressed: onOpen,
            child: const Text('Revisar borrador'),
          ),
        ],
      ),
    );
  }
}
