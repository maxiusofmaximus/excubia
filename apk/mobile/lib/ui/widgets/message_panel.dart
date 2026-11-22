import 'package:flutter/material.dart';

import '../theme.dart';

/// A centred explanation with one action (and an optional second one). Used
/// for the states of the agent list that have nothing to show.
class MessagePanel extends StatelessWidget {
  const MessagePanel({
    super.key,
    required this.icon,
    required this.title,
    required this.detail,
    required this.actionLabel,
    required this.onAction,
    this.hint,
    this.secondaryLabel,
    this.onSecondary,
  });

  final IconData icon;
  final String title;
  final String detail;
  final String? hint;
  final String actionLabel;
  final VoidCallback onAction;
  final String? secondaryLabel;
  final VoidCallback? onSecondary;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(Spacing.xl),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 40, color: context.colors.textMuted),
            const SizedBox(height: Spacing.md),
            Text(title, style: context.text.title, textAlign: TextAlign.center),
            const SizedBox(height: Spacing.sm),
            Text(
              detail,
              style: context.text.bodySecondary,
              textAlign: TextAlign.center,
            ),
            if (hint case final hint?) ...[
              const SizedBox(height: Spacing.sm),
              Text(hint, style: context.text.small, textAlign: TextAlign.center),
            ],
            const SizedBox(height: Spacing.lg),
            FilledButton(onPressed: onAction, child: Text(actionLabel)),
            if (secondaryLabel case final label? when onSecondary != null) ...[
              const SizedBox(height: Spacing.sm),
              OutlinedButton(onPressed: onSecondary, child: Text(label)),
            ],
          ],
        ),
      ),
    );
  }
}

/// The device is known to the host but no one has approved it yet. The panel
/// says so in plain words; a retry is one attempt, never a loop.
class AwaitingApprovalPanel extends StatelessWidget {
  const AwaitingApprovalPanel({super.key, required this.onRetry});

  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    return MessagePanel(
      icon: Icons.hourglass_top,
      title: 'Esperando que aprueben este dispositivo',
      detail:
          'Abre el equipo y aprueba este teléfono. '
          'Cuando lo hayas hecho, pulsa Reintentar.',
      actionLabel: 'Reintentar',
      onAction: onRetry,
    );
  }
}
