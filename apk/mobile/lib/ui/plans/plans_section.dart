import 'package:flutter/material.dart';

import '../../core/api/plan_models.dart';
import '../../core/api/plans_api.dart';
import '../../core/state/plans_controller.dart';
import '../team/team_panel_parts.dart';
import '../theme.dart';
import '../widgets/empty_state.dart';
import 'plan_card.dart';
import 'plan_dialogs.dart';
import 'plan_text.dart';

/// «Planes propuestos» on the Equipo tab, above «Tu equipo». The controller decides when to
/// read; this widget shows what it holds and asks the user before each answer.
class PlansSection extends StatelessWidget {
  const PlansSection({super.key, required this.plans});

  final PlansController plans;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: plans,
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
            if (plans.denied) ...[
              TeamNoticeBox(text: planPermissionText),
              const SizedBox(height: Spacing.sm),
            ],
            _body(context),
          ],
        ),
      ),
    );
  }

  Widget _header(BuildContext context) {
    final pending = plans.pendingCount;
    return Row(
      children: [
        Expanded(child: Text('PLANES PROPUESTOS', style: context.text.label)),
        if (pending > 0)
          Flexible(
            child: Semantics(
              liveRegion: true,
              child: Text(
                planWaitingText(pending),
                textAlign: TextAlign.end,
                style: context.text.small.copyWith(
                  color: context.colors.accentText,
                ),
              ),
            ),
          ),
      ],
    );
  }

  Widget _body(BuildContext context) {
    final error = plans.error;
    if (!plans.loaded && error == null) {
      return const TeamLoadingBlock(label: planLoadingLabel);
    }
    final list = plans.plans;
    if (list.isEmpty) {
      if (error != null) return _noticeFor(error);
      return const EmptyState(
        icon: Icons.assignment_outlined,
        title: planEmptyTitle,
        detail: planEmptyDetail,
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (error != null) ...[
          _noticeFor(error),
          const SizedBox(height: Spacing.sm),
        ],
        for (final plan in list) ...[
          PlanCard(
            plan: plan,
            busy: plans.isBusy(plan.id),
            locked: plans.denied,
            failure: _failureText(plan.id),
            onApprove: () => _approve(context, plan),
            onChange: () => _change(context, plan),
            onReject: () => _reject(context, plan),
          ),
          const SizedBox(height: Spacing.sm),
        ],
      ],
    );
  }

  String? _failureText(String planId) {
    final failure = plans.failureOf(planId);
    return failure == null ? null : planDecisionFailureText(failure);
  }

  TeamNoticeBox _noticeFor(Object error) => TeamNoticeBox(
    text: planReadFailureText(error),
    onRetry: planReadWaitingText(error) == null
        ? () => plans.refresh()
        : null,
  );

  Future<void> _approve(BuildContext context, TeamPlan plan) async {
    if (needsExtraConfirm(plan) && !await confirmApprovePlan(context)) return;
    await plans.decide(plan, PlanDecision.approve);
  }

  Future<void> _change(BuildContext context, TeamPlan plan) async {
    final note = await askPlanChanges(context);
    if (note == null) return;
    await plans.decide(plan, PlanDecision.requestChanges, note: note);
  }

  Future<void> _reject(BuildContext context, TeamPlan plan) async {
    if (!await confirmRejectPlan(context)) return;
    await plans.decide(plan, PlanDecision.reject);
  }
}
