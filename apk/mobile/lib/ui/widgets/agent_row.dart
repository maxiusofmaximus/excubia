import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/state.dart';

import '../theme.dart';
import 'status_dot.dart';

/// One agent: project and task, one line each, the whole row tappable.
class AgentRow extends StatelessWidget {
  const AgentRow({super.key, required this.agent, required this.onTap});

  final AgentEntry agent;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final blocked = agent.status == AgentStatus.blocked;
    final detail = agent.title?.trim();
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: Spacing.lg,
        vertical: Spacing.xs,
      ),
      child: Material(
        color: blocked
            ? Color.alphaBlend(
                context.colors.statusBlocked.withValues(alpha: 0.08),
                context.colors.raised,
              )
            : context.colors.surface,
        borderRadius: BorderRadius.circular(12),
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(12),
          child: Container(
            constraints: const BoxConstraints(minHeight: 64),
            padding: const EdgeInsets.symmetric(
              horizontal: Spacing.md,
              vertical: Spacing.md,
            ),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(12),
              border: Border.all(
                color: blocked
                    ? context.colors.statusBlocked.withValues(alpha: 0.55)
                    : context.colors.outlineSoft,
              ),
            ),
            child: Row(
              children: [
                StatusDot(status: agent.status),
                const SizedBox(width: Spacing.md),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      Text(
                        agent.project,
                        style: context.text.title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      const SizedBox(height: 2),
                      Text(
                        detail == null || detail.isEmpty
                            ? context.colors.statusLabel(agent.status)
                            : '${context.colors.statusLabel(agent.status)} · $detail',
                        style: context.text.small.copyWith(
                          color: context.colors.labelForStatus(agent.status),
                        ),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: Spacing.sm),
                Icon(
                  Icons.chevron_right,
                  color: context.colors.textMuted,
                  size: 20,
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
