import 'package:flutter/material.dart';

import '../../core/api/schedule_models.dart';
import '../theme.dart';
import 'schedule_text.dart';

/// One scheduled task: its oficio and rule in a sentence, its next and last run, the pause
/// switch, «Lanzar ahora» and «Historial». The text of the task is never shown here.
class ScheduleCard extends StatelessWidget {
  const ScheduleCard({
    super.key,
    required this.schedule,
    required this.oficio,
    required this.busy,
    required this.locked,
    required this.onChange,
    required this.onRun,
    required this.onHistory,
    this.note,
  });

  final Schedule schedule;

  /// The oficio's name, as the team shows it.
  final String oficio;

  /// A change to this task is in flight.
  final bool busy;

  /// The device may not change tasks (no `APPROVE`): the actions stay visible but off.
  final bool locked;

  /// The pause switch asks for the opposite of the state it shows.
  final ValueChanged<ScheduleChange> onChange;
  final VoidCallback onRun;
  final VoidCallback onHistory;

  /// The words of the last change: a refusal, or what «Lanzar ahora» answered.
  final String? note;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final text = context.text;
    final on = !busy && !locked;
    final message = busy ? 'Enviando…' : note;
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
          Text(schedule.title, style: text.title),
          const SizedBox(height: Spacing.xs),
          Text(
            '$oficio · ${scheduleRuleText(schedule)}',
            style: text.bodySecondary,
          ),
          const SizedBox(height: Spacing.sm),
          Text(
            scheduleNextText(schedule.nextRunAt, schedule.status),
            style: text.small,
          ),
          Text(scheduleLastText(schedule.lastRunAt), style: text.small),
          const SizedBox(height: Spacing.sm),
          _StatusRow(status: schedule.status, enabled: on, onChange: onChange),
          const SizedBox(height: Spacing.sm),
          Wrap(
            spacing: Spacing.sm,
            runSpacing: Spacing.xs,
            children: [
              OutlinedButton(
                style: OutlinedButton.styleFrom(minimumSize: const Size(0, 48)),
                onPressed: on ? onRun : null,
                child: const Text('Lanzar ahora'),
              ),
              TextButton(
                style: TextButton.styleFrom(minimumSize: const Size(48, 48)),
                onPressed: onHistory,
                child: const Text('Historial'),
              ),
            ],
          ),
          if (message != null) ...[
            const SizedBox(height: Spacing.xs),
            Semantics(
              liveRegion: true,
              child: Text(
                message,
                style: text.small.copyWith(
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

/// The word for the state and the switch that pauses or resumes an active or paused task.
/// A finished task has no switch: there is nothing to pause.
class _StatusRow extends StatelessWidget {
  const _StatusRow({
    required this.status,
    required this.enabled,
    required this.onChange,
  });

  final ScheduleStatus status;
  final bool enabled;
  final ValueChanged<ScheduleChange> onChange;

  @override
  Widget build(BuildContext context) {
    final active = status == ScheduleStatus.active;
    return Row(
      children: [
        Expanded(
          child: Text(scheduleStatusLabel(status), style: context.text.body),
        ),
        if (status != ScheduleStatus.done)
          Switch(
            value: active,
            onChanged: enabled
                ? (on) => onChange(
                    on ? ScheduleChange.resume : ScheduleChange.pause,
                  )
                : null,
          ),
      ],
    );
  }
}
