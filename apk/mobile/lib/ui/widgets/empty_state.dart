import 'package:flutter/material.dart';

import '../theme.dart';

/// An empty list said lightly: an icon, a line of words and at most one quiet
/// text action. The screen's primary button is never repeated here.
class EmptyState extends StatelessWidget {
  const EmptyState({
    super.key,
    required this.icon,
    required this.title,
    required this.detail,
    this.actionLabel,
    this.onAction,
  });

  final IconData icon;
  final String title;
  final String detail;
  final String? actionLabel;
  final VoidCallback? onAction;

  @override
  Widget build(BuildContext context) {
    final action = actionLabel;
    final onTap = onAction;
    return Padding(
      padding: const EdgeInsets.all(Spacing.xl),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 32, color: context.colors.textMuted),
          const SizedBox(height: Spacing.md),
          Text(title, style: context.text.title, textAlign: TextAlign.center),
          const SizedBox(height: Spacing.xs),
          Text(
            detail,
            style: context.text.bodySecondary,
            textAlign: TextAlign.center,
          ),
          if (action != null && onTap != null) ...[
            const SizedBox(height: Spacing.sm),
            TextButton(onPressed: onTap, child: Text(action)),
          ],
        ],
      ),
    );
  }
}
