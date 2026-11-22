import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/schedule_models.dart';

Map<String, Object?> _schedule({
  String id = 'sch_1',
  String status = 'active',
  String? rule,
  String? nextRunAt = '2026-11-06T08:00:00.000Z',
  String? lastRunAt,
}) => {
  'id': id,
  'title': 'Resumen del lunes',
  'conversation_id': 'conv_1',
  'role_id': 'audio-editor',
  'body': 'texto del encargo que no se muestra',
  'recipe_id': null,
  'rule': rule,
  'start_date': '2026-11-02',
  'time_local': '09:00',
  'timezone': 'Europe/Madrid',
  'steps': 1,
  'status': status,
  'plan_id': null,
  'next_run_at': nextRunAt,
  'last_run_at': lastRunAt,
  'created_at': '2026-11-01T10:00:00.000Z',
  'updated_at': '2026-11-01T10:00:00.000Z',
};

Map<String, Object?> _run(String id, String dueAt, String outcome) => {
  'id': id,
  'schedule_id': 'sch_1',
  'due_at': dueAt,
  'started_at': null,
  'outcome': outcome,
  'delegation_id': null,
};

void main() {
  group('reading a schedule', () {
    test('keeps the fields the screen shows and turns the instants local', () {
      final schedule = Schedule.fromJson(
        _schedule(
          rule: 'FREQ=WEEKLY;INTERVAL=1;BYDAY=MO',
          lastRunAt: '2026-11-02T08:00:00.000Z',
        ),
      );

      expect(schedule.id, 'sch_1');
      expect(schedule.title, 'Resumen del lunes');
      expect(schedule.roleId, 'audio-editor');
      expect(schedule.rule, 'FREQ=WEEKLY;INTERVAL=1;BYDAY=MO');
      expect(schedule.timeLocal, '09:00');
      expect(schedule.status, ScheduleStatus.active);
      expect(
        schedule.nextRunAt,
        DateTime.parse('2026-11-06T08:00:00.000Z').toLocal(),
      );
      expect(
        schedule.lastRunAt,
        DateTime.parse('2026-11-02T08:00:00.000Z').toLocal(),
      );
    });

    test('a missing instant and a missing rule read as none', () {
      final schedule = Schedule.fromJson(
        _schedule(nextRunAt: null, lastRunAt: null),
      );

      expect(schedule.nextRunAt, isNull);
      expect(schedule.lastRunAt, isNull);
      expect(schedule.rule, isNull);
    });

    test('an unknown status is a broken answer, not an empty card', () {
      expect(
        () => Schedule.fromJson(_schedule(status: 'quizá')),
        throwsFormatException,
      );
    });

    test('a missing required field is a broken answer', () {
      final json = _schedule()..remove('role_id');

      expect(() => Schedule.fromJson(json), throwsFormatException);
    });
  });

  group('ordering the schedules', () {
    test('active first, then paused, then done; by next run inside each', () {
      final done = Schedule.fromJson(
        _schedule(id: 'd', status: 'done', nextRunAt: null),
      );
      final paused = Schedule.fromJson(
        _schedule(id: 'p', status: 'paused', nextRunAt: null),
      );
      final later = Schedule.fromJson(
        _schedule(id: 'l', nextRunAt: '2026-11-09T08:00:00.000Z'),
      );
      final sooner = Schedule.fromJson(
        _schedule(id: 's', nextRunAt: '2026-11-06T08:00:00.000Z'),
      );

      final ids = sortSchedules([done, later, paused, sooner]).map((s) => s.id);

      expect(ids.toList(), ['s', 'l', 'p', 'd']);
    });
  });

  group('reading the runs', () {
    test('every outcome the host can answer is read', () {
      final runs = runsFrom({
        'ok': true,
        'runs': [
          _run('r1', '2026-11-02T08:00:00.000Z', 'started'),
          _run('r2', '2026-11-03T08:00:00.000Z', 'skipped_cap'),
          _run('r3', '2026-11-04T08:00:00.000Z', 'skipped_overlap'),
          _run('r4', '2026-11-05T08:00:00.000Z', 'missed_offline'),
          _run('r5', '2026-11-06T08:00:00.000Z', 'failed'),
        ],
      });

      expect(runs.map((run) => run.outcome).toList(), [
        RunOutcome.failed,
        RunOutcome.missedOffline,
        RunOutcome.skippedOverlap,
        RunOutcome.skippedCap,
        RunOutcome.started,
      ]);
    });

    test('the newest run comes first', () {
      final runs = runsFrom({
        'ok': true,
        'runs': [
          _run('old', '2026-11-02T08:00:00.000Z', 'started'),
          _run('new', '2026-11-05T08:00:00.000Z', 'started'),
        ],
      });

      expect(runs.map((run) => run.id).toList(), ['new', 'old']);
    });

    test('an unknown outcome is a broken answer', () {
      expect(
        () => runsFrom({
          'ok': true,
          'runs': [_run('r1', '2026-11-02T08:00:00.000Z', 'inventado')],
        }),
        throwsFormatException,
      );
    });
  });

  group('the answer to «Lanzar ahora»', () {
    test('without a run it was launched', () {
      final launch = ScheduleLaunch.fromJson({
        'ok': true,
        'delegation_id': 'del_1',
        'status': 'queued',
        'depth': 1,
      });

      expect(launch.launched, isTrue);
      expect(launch.skipped, isNull);
    });

    test('with a recorded run it was skipped by that brake', () {
      final launch = ScheduleLaunch.fromJson({
        'ok': true,
        'run': _run('r1', '2026-11-02T08:00:00.000Z', 'skipped_cap'),
      });

      expect(launch.launched, isFalse);
      expect(launch.skipped, RunOutcome.skippedCap);
    });
  });
}
