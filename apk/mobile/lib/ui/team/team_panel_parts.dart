import 'package:flutter/material.dart';

import '../theme.dart';

/// Two placeholder bars while the first read of a team panel is out.
class TeamLoadingBlock extends StatelessWidget {
  const TeamLoadingBlock({super.key, required this.label});

  final String label;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Semantics(
      label: label,
      child: Column(
        children: [
          for (var i = 0; i < 2; i++)
            Container(
              height: 56,
              margin: const EdgeInsets.only(bottom: Spacing.sm),
              decoration: BoxDecoration(
                color: c.raised,
                borderRadius: BorderRadius.circular(16),
              ),
            ),
        ],
      ),
    );
  }
}

/// A failed read, said once. [onRetry] adds the way out; null when there is none to offer.
/// The cards already on screen stay under it.
class TeamNoticeBox extends StatelessWidget {
  const TeamNoticeBox({super.key, required this.text, this.onRetry});

  final String text;
  final VoidCallback? onRetry;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final retry = onRetry;
    return Semantics(
      liveRegion: true,
      child: Container(
        padding: const EdgeInsets.fromLTRB(
          Spacing.md,
          Spacing.sm,
          Spacing.xs,
          Spacing.sm,
        ),
        decoration: BoxDecoration(
          color: c.accentSoft,
          borderRadius: BorderRadius.circular(12),
        ),
        child: Row(
          children: [
            Icon(Icons.error_outline, size: 20, color: c.accentText),
            const SizedBox(width: Spacing.sm),
            Expanded(child: Text(text, style: context.text.body)),
            if (retry != null)
              TextButton(
                style: TextButton.styleFrom(minimumSize: const Size(48, 48)),
                onPressed: retry,
                child: const Text('Reintentar'),
              ),
          ],
        ),
      ),
    );
  }
}
