import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/error.dart';

import '../theme.dart';

/// The red-bordered message plus the action the Rust core supplied.
class ErrorBox extends StatelessWidget {
  const ErrorBox({super.key, required this.error});

  final ErrorInfo error;

  @override
  Widget build(BuildContext context) {
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
            Icons.error_outline,
            color: context.colors.statusBlocked,
            size: 20,
          ),
          const SizedBox(width: Spacing.sm),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(error.message, style: context.text.body),
                if (error.action != null) ...[
                  const SizedBox(height: Spacing.xs),
                  Text(error.action!, style: context.text.small),
                ],
              ],
            ),
          ),
        ],
      ),
    );
  }
}
