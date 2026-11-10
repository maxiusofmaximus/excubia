import 'package:flutter/material.dart';
import 'package:excubia/src/rust/api/onboarding/model.dart';

import '../../core/api/team_summary_models.dart';
import '../theme.dart';
import '../widgets/agent_avatar.dart';
import 'team_summary_text.dart';

/// One person of «Tu equipo»: avatar, name and role, the state with its age, and the host's
/// sentence. Those waiting for the user are tinted, not bordered. [hire] supplies the face the
/// user chose; a person without one shows the initial.
class TeamSummaryCard extends StatelessWidget {
  const TeamSummaryCard({
    super.key,
    required this.member,
    required this.age,
    this.hire,
    this.onTap,
  });

  final TeamMember member;

  /// «desde hace 12 min», or null when the host sent no time.
  final String? age;
  final Hire? hire;

  /// Null when the person cannot be opened (no `can_hire`).
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final waiting = member.status == TeamStatus.waitingForYou;
    final label = teamStatusLabel(member.status);
    final headline = teamShownHeadline(member.headline, label);
    return Semantics(
      button: onTap != null,
      label: _spoken(label, headline),
      excludeSemantics: true,
      child: Material(
        color: waiting ? c.accentSoft : Colors.transparent,
        child: InkWell(
          onTap: onTap,
          child: Container(
            constraints: const BoxConstraints(minHeight: 56),
            padding: const EdgeInsets.symmetric(
              horizontal: Spacing.lg,
              vertical: Spacing.md,
            ),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                AgentAvatar(
                  name: member.name,
                  characterId: hire?.avatar,
                  faceFromName: false,
                  animated: false,
                ),
                const SizedBox(width: Spacing.md),
                Expanded(child: _text(context, c, waiting, label, headline)),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _text(
    BuildContext context,
    HerdrColors c,
    bool waiting,
    String label,
    String headline,
  ) {
    final role = member.roleTitle;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Wrap(
          crossAxisAlignment: WrapCrossAlignment.center,
          spacing: Spacing.sm,
          children: [
            Text(
              member.name,
              style: context.text.title,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
            if (role.isNotEmpty) Text(role, style: context.text.small),
          ],
        ),
        const SizedBox(height: Spacing.xs),
        Wrap(
          crossAxisAlignment: WrapCrossAlignment.center,
          spacing: Spacing.xs,
          children: [
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Container(
                  width: 8,
                  height: 8,
                  decoration: BoxDecoration(
                    color: _toneOf(c, member.status),
                    shape: BoxShape.circle,
                  ),
                ),
                const SizedBox(width: Spacing.xs),
                Flexible(
                  child: Text(
                    label,
                    style: context.text.label.copyWith(
                      color: waiting ? c.accentText : c.textSecondary,
                    ),
                  ),
                ),
              ],
            ),
            if (age != null) Text('· $age', style: context.text.small),
          ],
        ),
        if (headline.isNotEmpty) ...[
          const SizedBox(height: Spacing.xs),
          Text(headline, style: context.text.bodySecondary),
        ],
      ],
    );
  }

  String _spoken(String label, String headline) {
    final parts = [
      member.name,
      if (member.roleTitle.isNotEmpty) member.roleTitle,
      label,
      ?age,
      if (headline.isNotEmpty) headline,
    ];
    return parts.join(', ');
  }
}

Color _toneOf(HerdrColors c, TeamStatus status) => switch (status) {
  TeamStatus.waitingForYou => c.accent,
  TeamStatus.blocked => c.statusBlocked,
  TeamStatus.working => c.statusWorking,
  TeamStatus.done => c.statusDone,
  TeamStatus.idle => c.statusIdle,
};
