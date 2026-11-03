import 'package:flutter/material.dart';

import '../theme.dart';

/// The pairing failure card. [message] is the answer in plain Spanish; the
/// technical [detail] of a pinned refusal stays folded below it.
class PairingErrorBox extends StatelessWidget {
  const PairingErrorBox({
    super.key,
    required this.message,
    this.action,
    this.detail,
    this.security = false,
  });

  final String message;
  final String? action;
  final String? detail;

  /// A host that could not be verified: another icon, so it is not read as a
  /// plain failure to retry.
  final bool security;

  @override
  Widget build(BuildContext context) {
    final technical = detail;
    return Container(
      decoration: BoxDecoration(
        color: context.colors.surface,
        borderRadius: BorderRadius.circular(10),
        border: Border.all(
          color: context.colors.statusBlocked.withValues(alpha: 0.6),
        ),
      ),
      padding: const EdgeInsets.all(Spacing.md),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(
            security ? Icons.gpp_bad_outlined : Icons.error_outline,
            color: context.colors.statusBlocked,
            size: 20,
          ),
          const SizedBox(width: Spacing.sm),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(message, style: context.text.body),
                if (action != null) ...[
                  const SizedBox(height: Spacing.xs),
                  Text(action!, style: context.text.small),
                ],
                if (technical != null && technical.isNotEmpty)
                  ExpansionTile(
                    tilePadding: EdgeInsets.zero,
                    childrenPadding: EdgeInsets.zero,
                    expandedCrossAxisAlignment: CrossAxisAlignment.start,
                    shape: const Border(),
                    collapsedShape: const Border(),
                    title: Text('Detalle técnico', style: context.text.small),
                    children: [
                      SelectableText(technical, style: context.text.mono),
                    ],
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
