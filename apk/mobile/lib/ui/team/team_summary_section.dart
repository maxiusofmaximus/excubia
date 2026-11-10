import 'package:flutter/material.dart';
import 'package:excubia/src/rust/api/onboarding/model.dart';

import '../../core/api/team_summary_api.dart';
import '../../core/api/team_summary_models.dart';
import '../../core/state/hires_controller.dart';
import '../../core/state/plans_controller.dart';
import '../../core/state/recipes_controller.dart';
import '../../core/state/team_summary_controller.dart';
import '../plans/plan_text.dart';
import '../theme.dart';
import '../widgets/empty_state.dart';
import '../widgets/grouped_card.dart';
import 'hire_actions.dart';
import 'hires_section.dart';
import 'team_panel_parts.dart';
import 'team_summary_card.dart';
import 'team_summary_text.dart';

/// «Tu equipo» at the top of the Equipo tab: one card per person with what they are doing.
/// The screen reads what the summary controller holds; the controller decides when to read.
class TeamSummarySection extends StatelessWidget {
  const TeamSummarySection({
    super.key,
    required this.summary,
    required this.hires,
    required this.recipes,
    required this.plans,
  });

  final TeamSummaryController summary;
  final HiresController hires;
  final RecipesController recipes;
  final PlansController plans;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Listenable.merge([summary, hires, plans]),
      builder: (context, _) => Padding(
        padding: const EdgeInsets.fromLTRB(
          Spacing.lg,
          Spacing.sm,
          Spacing.lg,
          Spacing.sm,
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _header(context),
            const SizedBox(height: Spacing.sm),
            _body(context),
          ],
        ),
      ),
    );
  }

  /// What the counter says. A failed read does not know who waits, so it stays empty.
  String _counterText(int waiting, int pending) {
    if (summary.error != null) return '';
    if (waiting == 0 && pending > 0) return planWaitingText(pending);
    return teamWaitingText(waiting);
  }

  /// Someone on the team waits for the user first; a plan waits when nobody else does.
  Widget _header(BuildContext context) {
    final c = context.colors;
    final waiting = summary.waitingCount;
    final pending = plans.pendingCount;
    final onlyPlans = waiting == 0 && pending > 0;
    return Row(
      children: [
        Expanded(child: Text('TU EQUIPO', style: context.text.label)),
        Flexible(
          child: Semantics(
            liveRegion: true,
            child: Text(
              _counterText(waiting, pending),
              textAlign: TextAlign.end,
              style: context.text.small.copyWith(
                color: waiting > 0 || onlyPlans
                    ? c.accentText
                    : c.textSecondary,
              ),
            ),
          ),
        ),
      ],
    );
  }

  Widget _body(BuildContext context) {
    final error = summary.error;
    if (!summary.loaded && error == null) {
      return const TeamLoadingBlock(label: 'Cargando tu equipo');
    }
    if (summary.members.isEmpty) {
      if (error != null) return _noticeFor(error);
      return EmptyState(
        icon: Icons.groups_outlined,
        title: 'Todavía no tienes a nadie contratado.',
        detail: 'Contrata a quien necesites y verás aquí en qué anda cada uno.',
        actionLabel: 'Contratar',
        onAction: hires.canHire && !hires.busy
            ? () => openHireCatalog(context, hires)
            : null,
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (error != null) ...[
          _noticeFor(error),
          const SizedBox(height: Spacing.sm),
        ],
        GroupedCard(
          children: [
            for (final member in summary.members) _cardFor(context, member),
          ],
        ),
      ],
    );
  }

  /// A failed read: the wait for the grant has no way out; any other failure can be read again.
  TeamNoticeBox _noticeFor(Object error) => TeamNoticeBox(
    text: teamSummaryFailureText(error),
    onRetry: teamSummaryWaitingText(error) == null
        ? () => summary.refresh()
        : null,
  );

  Widget _cardFor(BuildContext context, TeamMember member) {
    final hire = _hireFor(member.agentId);
    final open = hire != null && hires.canHire;
    return TeamSummaryCard(
      member: member,
      hire: hire,
      age: teamAgeLabel(member.since, DateTime.now()),
      onTap: open
          ? () => showHireActions(
              context,
              hires,
              hire,
              recipes,
              onHire: hireCatalogAction(context, hires),
            )
          : null,
    );
  }

  /// The contracted person behind a card. Null when the host names one not on this device's list.
  Hire? _hireFor(String agentId) {
    for (final hire in hires.hires) {
      if (hire.id == agentId) return hire;
    }
    return null;
  }
}
