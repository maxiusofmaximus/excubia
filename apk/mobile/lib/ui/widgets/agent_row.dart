import 'package:flutter/material.dart';
import 'package:excubia/src/rust/api/state.dart';

import '../theme.dart';
import 'agent_avatar.dart';
import 'agent_text.dart';

/// One agent: project and task, one line each, the whole row tappable.
class AgentRow extends StatelessWidget {
  const AgentRow({super.key, required this.agent, required this.onTap});

  final AgentEntry agent;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final blocked = agent.status == AgentStatus.blocked;
    final subtitle = agentSubtitle(
      agent.status,
      context.colors.statusLabel(agent.status),
      agent.title,
    );
    // A finished agent with no task line says when it ended, so two look different.
    final ended = subtitle == null ? agentEndedText(agent) : null;
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
                AgentAvatar(
                  name: agentNameOf(agent),
                  status: agent.status,
                  size: 40,
                  ringed: true,
                ),
                const SizedBox(width: Spacing.md),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      Text(
                        agentProjectLabel(agent),
                        style: context.text.title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      const SizedBox(height: 2),
                      // The word carries the state; the point only repeats it.
                      if (subtitle != null)
                        Row(
                          children: [
                            Container(
                              width: 8,
                              height: 8,
                              decoration: BoxDecoration(
                                shape: BoxShape.circle,
                                color: context.colors.forStatus(agent.status),
                              ),
                            ),
                            const SizedBox(width: Spacing.xs),
                            Expanded(
                              child: Text(
                                subtitle,
                                style: context.text.small.copyWith(
                                  color: context.colors.labelForStatus(
                                    agent.status,
                                  ),
                                ),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                              ),
                            ),
                          ],
                        )
                      else if (ended != null)
                        Text(
                          ended,
                          style: context.text.small.copyWith(
                            color: context.colors.textMuted,
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
