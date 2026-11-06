import 'package:flutter/material.dart';

import '../../core/api/plan_models.dart';
import '../theme.dart';
import 'plan_text.dart';

/// One plan: the host's sentence, its numbered steps with what they touch, and the three
/// answers while it is open. A decided plan keeps its status and note, with no answers.
class PlanCard extends StatelessWidget {
  const PlanCard({
    super.key,
    required this.plan,
    required this.busy,
    required this.locked,
    required this.onApprove,
    required this.onChange,
    required this.onReject,
    this.failure,
  });

  final TeamPlan plan;

  /// A decision for this plan is in flight.
  final bool busy;

  /// The device may not decide (no `APPROVE`): the answers stay visible but off.
  final bool locked;

  /// The words of the last refusal, or null.
  final String? failure;

  final VoidCallback onApprove;
  final VoidCallback onChange;
  final VoidCallback onReject;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final text = context.text;
    final decided = !plan.isProposed;
    final ink = decided ? c.textSecondary : c.textPrimary;
    final title = plan.summary.isEmpty
        ? planStepsLine(plan.steps)
        : plan.summary;
    return Container(
      padding: const EdgeInsets.all(Spacing.lg),
      decoration: BoxDecoration(
        color: c.raised,
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: c.outlineSoft),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(title, style: text.title.copyWith(color: ink)),
          const SizedBox(height: Spacing.sm),
          for (final (index, step) in plan.steps.indexed) ...[
            _StepLine(number: index + 1, step: step, ink: ink),
            const SizedBox(height: Spacing.xs),
          ],
          if (decided)
            _Decided(plan: plan)
          else
            _Answers(
              busy: busy,
              locked: locked,
              failure: failure,
              onApprove: onApprove,
              onChange: onChange,
              onReject: onReject,
            ),
        ],
      ),
    );
  }
}

class _StepLine extends StatelessWidget {
  const _StepLine({required this.number, required this.step, required this.ink});

  final int number;
  final PlanStep step;
  final Color ink;

  @override
  Widget build(BuildContext context) {
    final text = context.text;
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(
          width: 24,
          child: Text('$number.', style: text.bodySecondary),
        ),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(step.title, style: text.body.copyWith(color: ink)),
              Text(planStepTouchText(step), style: text.small),
            ],
          ),
        ),
      ],
    );
  }
}

/// A decided plan: its status, and the note the user sent with a change request.
class _Decided extends StatelessWidget {
  const _Decided({required this.plan});

  final TeamPlan plan;

  @override
  Widget build(BuildContext context) {
    final note = plan.note;
    return Padding(
      padding: const EdgeInsets.only(top: Spacing.sm),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(planStatusLabel(plan.status), style: context.text.label),
          if (note != null && note.isNotEmpty) ...[
            const SizedBox(height: Spacing.xs),
            Text('Tu nota: $note', style: context.text.bodySecondary),
          ],
        ],
      ),
    );
  }
}

class _Answers extends StatelessWidget {
  const _Answers({
    required this.busy,
    required this.locked,
    required this.failure,
    required this.onApprove,
    required this.onChange,
    required this.onReject,
  });

  final bool busy;
  final bool locked;
  final String? failure;
  final VoidCallback onApprove;
  final VoidCallback onChange;
  final VoidCallback onReject;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final on = !busy && !locked;
    final message = busy ? 'Enviando…' : failure;
    return Padding(
      padding: const EdgeInsets.only(top: Spacing.sm),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Wrap(
            spacing: Spacing.sm,
            runSpacing: Spacing.xs,
            children: [
              FilledButton(
                style: FilledButton.styleFrom(minimumSize: const Size(0, 48)),
                onPressed: on ? onApprove : null,
                child: const Text('Aprobar'),
              ),
              OutlinedButton(
                style: OutlinedButton.styleFrom(minimumSize: const Size(0, 48)),
                onPressed: on ? onChange : null,
                child: const Text('Cambiar…'),
              ),
              TextButton(
                style: TextButton.styleFrom(
                  minimumSize: const Size(48, 48),
                  foregroundColor: c.statusBlocked,
                ),
                onPressed: on ? onReject : null,
                child: const Text('Rechazar'),
              ),
            ],
          ),
          if (message != null) ...[
            const SizedBox(height: Spacing.xs),
            Semantics(
              liveRegion: true,
              child: Text(
                message,
                style: context.text.small.copyWith(
                  color: busy ? c.textSecondary : c.accentText,
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }
}
