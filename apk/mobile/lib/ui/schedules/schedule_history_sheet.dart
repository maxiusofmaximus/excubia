import 'package:flutter/material.dart';

import '../../core/api/schedule_models.dart';
import '../../core/api/schedules_api.dart';
import '../team/team_panel_parts.dart';
import '../theme.dart';
import 'schedule_text.dart';

/// The last runs of one task, read only, newest first. It shows when each was due and what
/// became of it; the text of the task is not part of it.
Future<void> showScheduleHistory(
  BuildContext context, {
  required SchedulesApi api,
  required Schedule schedule,
}) => showModalBottomSheet<void>(
  context: context,
  showDragHandle: true,
  isScrollControlled: true,
  builder: (_) => _HistorySheet(api: api, schedule: schedule),
);

class _HistorySheet extends StatefulWidget {
  const _HistorySheet({required this.api, required this.schedule});

  final SchedulesApi api;
  final Schedule schedule;

  @override
  State<_HistorySheet> createState() => _HistorySheetState();
}

class _HistorySheetState extends State<_HistorySheet> {
  late Future<List<ScheduleRun>> _runs;

  @override
  void initState() {
    super.initState();
    _runs = widget.api.runs(widget.schedule.id);
  }

  void _reload() => setState(() => _runs = widget.api.runs(widget.schedule.id));

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(
          Spacing.lg,
          0,
          Spacing.lg,
          Spacing.lg,
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text('Historial', style: context.text.title),
            Text(widget.schedule.title, style: context.text.bodySecondary),
            const SizedBox(height: Spacing.md),
            FutureBuilder<List<ScheduleRun>>(
              future: _runs,
              builder: (context, snapshot) => _content(context, snapshot),
            ),
            const SizedBox(height: Spacing.lg),
            Align(
              alignment: Alignment.centerRight,
              child: TextButton(
                style: TextButton.styleFrom(minimumSize: const Size(48, 48)),
                onPressed: () => Navigator.pop(context),
                child: const Text('Cerrar'),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _content(
    BuildContext context,
    AsyncSnapshot<List<ScheduleRun>> snapshot,
  ) {
    if (snapshot.connectionState != ConnectionState.done) {
      return const TeamLoadingBlock(label: scheduleLoadingLabel);
    }
    final error = snapshot.error;
    if (error != null) {
      return TeamNoticeBox(
        text: scheduleReadFailureText(error),
        onRetry: scheduleReadWaitingText(error) == null ? _reload : null,
      );
    }
    final runs = snapshot.data ?? const <ScheduleRun>[];
    if (runs.isEmpty) {
      return Text(scheduleHistoryEmptyText, style: context.text.bodySecondary);
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [for (final run in runs) _RunRow(run: run)],
    );
  }
}

class _RunRow extends StatelessWidget {
  const _RunRow({required this.run});

  final ScheduleRun run;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: Spacing.sm),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(scheduleWhenText(run.dueAt), style: context.text.body),
          Text(runOutcomeLabel(run.outcome), style: context.text.small),
        ],
      ),
    );
  }
}
