import 'json_read.dart';

// The scheduled work of «Automatizar» (contract `api.md`, F4). The host keeps the schedules and
// their last 50 runs; a device reads them with READ_STATE and pauses, resumes or launches one
// with APPROVE. The text of a task is not read here: nothing on this screen shows it.

enum ScheduleStatus { active, paused, done }

/// What the pause switch asks the host to do with a task.
enum ScheduleChange { pause, resume }

enum RunOutcome { started, skippedCap, skippedOverlap, missedOffline, failed }

ScheduleStatus _statusOf(String value) => switch (value) {
  'active' => ScheduleStatus.active,
  'paused' => ScheduleStatus.paused,
  'done' => ScheduleStatus.done,
  _ => throw const FormatException('respuesta inesperada'),
};

RunOutcome _outcomeOf(String value) => switch (value) {
  'started' => RunOutcome.started,
  'skipped_cap' => RunOutcome.skippedCap,
  'skipped_overlap' => RunOutcome.skippedOverlap,
  'missed_offline' => RunOutcome.missedOffline,
  'failed' => RunOutcome.failed,
  _ => throw const FormatException('respuesta inesperada'),
};

/// An instant the host sent in UTC, shown in this device's zone; null when none was sent.
DateTime? _instantOf(String? text) =>
    text == null ? null : DateTime.parse(text).toLocal();

/// One scheduled task, without its text. [timeLocal] is «HH:MM» in the task's own zone.
class Schedule {
  const Schedule({
    required this.id,
    required this.title,
    required this.roleId,
    required this.rule,
    required this.startDate,
    required this.timeLocal,
    required this.status,
    required this.nextRunAt,
    required this.lastRunAt,
  });

  factory Schedule.fromJson(Map<String, Object?> json) => Schedule(
    id: jsonText(json, 'id'),
    title: jsonText(json, 'title'),
    roleId: jsonText(json, 'role_id'),
    rule: jsonOptionalText(json, 'rule'),
    startDate: DateTime.parse(jsonText(json, 'start_date')),
    timeLocal: jsonText(json, 'time_local'),
    status: _statusOf(jsonText(json, 'status')),
    nextRunAt: _instantOf(jsonOptionalText(json, 'next_run_at')),
    lastRunAt: _instantOf(jsonOptionalText(json, 'last_run_at')),
  );

  final String id;
  final String title;
  final String roleId;

  /// RFC 5545 as the desktop wrote it, or null for a one-time task.
  final String? rule;
  final DateTime startDate;
  final String timeLocal;
  final ScheduleStatus status;
  final DateTime? nextRunAt;
  final DateTime? lastRunAt;
}

/// Active first, then paused, then done; inside each, the next run comes first.
List<Schedule> sortSchedules(Iterable<Schedule> schedules) =>
    List<Schedule>.of(schedules)..sort((a, b) {
      final byStatus = _rank(a.status).compareTo(_rank(b.status));
      return byStatus != 0 ? byStatus : _byNextRun(a.nextRunAt, b.nextRunAt);
    });

int _rank(ScheduleStatus status) => switch (status) {
  ScheduleStatus.active => 0,
  ScheduleStatus.paused => 1,
  ScheduleStatus.done => 2,
};

/// The soonest run first; a task with no next run goes last.
int _byNextRun(DateTime? next, DateTime? other) {
  if (next == null) return other == null ? 0 : 1;
  if (other == null) return -1;
  return next.compareTo(other);
}

/// The schedules of a `GET /v1/schedules` envelope, sorted for the list.
List<Schedule> schedulesFrom(Map<String, Object?> envelope) => sortSchedules([
  for (final item in jsonList(envelope, 'schedules'))
    Schedule.fromJson(jsonObject(item)),
]);

/// One due instant of a task and what became of it.
class ScheduleRun {
  const ScheduleRun({
    required this.id,
    required this.dueAt,
    required this.outcome,
  });

  factory ScheduleRun.fromJson(Map<String, Object?> json) => ScheduleRun(
    id: jsonText(json, 'id'),
    dueAt: DateTime.parse(jsonText(json, 'due_at')).toLocal(),
    outcome: _outcomeOf(jsonText(json, 'outcome')),
  );

  final String id;
  final DateTime dueAt;
  final RunOutcome outcome;
}

/// The runs of a `GET …/runs` envelope, the newest due instant first.
List<ScheduleRun> runsFrom(Map<String, Object?> envelope) => [
  for (final item in jsonList(envelope, 'runs'))
    ScheduleRun.fromJson(jsonObject(item)),
]..sort((a, b) => b.dueAt.compareTo(a.dueAt));

/// What «Lanzar ahora» answered. The host records a run only when a brake skipped it; a launch
/// it made answers with the delegation instead, so [skipped] is then null.
class ScheduleLaunch {
  const ScheduleLaunch.launched() : skipped = null;

  const ScheduleLaunch.skipped(RunOutcome outcome) : skipped = outcome;

  factory ScheduleLaunch.fromJson(Map<String, Object?> json) {
    final run = json['run'];
    if (run == null) return const ScheduleLaunch.launched();
    return ScheduleLaunch.skipped(
      ScheduleRun.fromJson(jsonObject(run)).outcome,
    );
  }

  /// The brake that stopped the launch, or null when it was launched.
  final RunOutcome? skipped;

  bool get launched => skipped == null;
}
