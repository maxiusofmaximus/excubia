import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/api/schedule_models.dart';
import '../../core/api/schedules_api.dart';
import '../../core/state/schedules_controller.dart';
import '../team/team_panel_parts.dart';
import '../theme.dart';
import '../widgets/empty_state.dart';
import 'schedule_card.dart';
import 'schedule_dialogs.dart';
import 'schedule_history_sheet.dart';
import 'schedule_text.dart';

/// «Automatizar» on the Equipo tab. The controller decides when to read; this widget shows what
/// it holds. It only pauses, resumes and launches: creating or editing a task is done on the
/// desktop, and the footer always says the PC must be on.
class SchedulesSection extends StatelessWidget {
  const SchedulesSection({
    super.key,
    required this.schedules,
    required this.oficioOf,
  });

  final SchedulesController schedules;

  /// The oficio's name for a role id, as the team shows it.
  final String Function(String roleId) oficioOf;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: schedules,
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
            Text('Automatizar', style: context.text.title),
            const SizedBox(height: Spacing.sm),
            if (schedules.denied) ...[
              const TeamNoticeBox(text: schedulePermissionText),
              const SizedBox(height: Spacing.sm),
            ],
            _body(context),
            const SizedBox(height: Spacing.sm),
            Text(scheduleFooterText, style: context.text.small),
          ],
        ),
      ),
    );
  }

  Widget _body(BuildContext context) {
    final error = schedules.error;
    if (!schedules.loaded && error == null) {
      return const TeamLoadingBlock(label: scheduleLoadingLabel);
    }
    final list = schedules.schedules;
    if (list.isEmpty) {
      if (error != null) return _noticeFor(error);
      return const EmptyState(
        icon: Icons.schedule_outlined,
        title: scheduleEmptyTitle,
        detail: scheduleEmptyDetail,
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (error != null) ...[
          _noticeFor(error),
          const SizedBox(height: Spacing.sm),
        ],
        for (final schedule in list) ...[
          ScheduleCard(
            schedule: schedule,
            oficio: oficioOf(schedule.roleId),
            busy: schedules.isBusy(schedule.id),
            locked: schedules.denied,
            note: _noteOf(schedule.id),
            onChange: (change) => schedules.changeStatus(schedule, change),
            onRun: () => _run(context, schedule),
            onHistory: () => showScheduleHistory(
              context,
              api: schedules.api,
              schedule: schedule,
            ),
          ),
          const SizedBox(height: Spacing.sm),
        ],
      ],
    );
  }

  /// The refusal of the last change, or what the last launch answered; null when neither.
  String? _noteOf(String scheduleId) {
    final failure = schedules.failureOf(scheduleId);
    if (failure != null) return scheduleActionFailureText(failure);
    final launch = schedules.launchOf(scheduleId);
    return launch == null ? null : scheduleLaunchText(launch);
  }

  TeamNoticeBox _noticeFor(Object error) => TeamNoticeBox(
    text: scheduleReadFailureText(error),
    onRetry: scheduleReadWaitingText(error) == null
        ? () => unawaited(schedules.refresh())
        : null,
  );

  Future<void> _run(BuildContext context, Schedule schedule) async {
    if (!await confirmRunNow(context, schedule.title)) return;
    await schedules.runNow(schedule);
  }
}
