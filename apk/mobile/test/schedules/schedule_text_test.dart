import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/schedule_models.dart';
import 'package:herdr_notify/ui/schedules/schedule_text.dart';

Schedule _schedule({
  String? rule,
  String startDate = '2026-11-12',
  String timeLocal = '09:00',
  String status = 'active',
  String? lastRunAt,
}) => Schedule.fromJson({
  'id': 'sch_1',
  'title': 'Resumen',
  'conversation_id': 'conv_1',
  'role_id': 'audio-editor',
  'body': 'texto',
  'recipe_id': null,
  'rule': rule,
  'start_date': startDate,
  'time_local': timeLocal,
  'timezone': 'Europe/Madrid',
  'steps': 1,
  'status': status,
  'plan_id': null,
  'next_run_at': null,
  'last_run_at': lastRunAt,
  'created_at': '2026-11-01T10:00:00.000Z',
  'updated_at': '2026-11-01T10:00:00.000Z',
});

void main() {
  group('the sentence of the rule', () {
    test('a one-time run names its date and its hour', () {
      expect(scheduleRuleText(_schedule()), 'El 12 de nov. a las 9:00');
    });

    test('a daily rule runs every day at its hour', () {
      expect(
        scheduleRuleText(
          _schedule(rule: 'FREQ=DAILY;INTERVAL=1', timeLocal: '18:30'),
        ),
        'Todos los días a las 18:30',
      );
    });

    test('a weekly rule names its days', () {
      expect(
        scheduleRuleText(_schedule(rule: 'FREQ=WEEKLY;INTERVAL=1;BYDAY=MO')),
        'Cada lunes a las 9:00',
      );
      expect(
        scheduleRuleText(
          _schedule(rule: 'FREQ=WEEKLY;INTERVAL=1;BYDAY=MO,WE,FR'),
        ),
        'Cada lunes, miércoles y viernes a las 9:00',
      );
    });

    test('a monthly rule keeps the day of its first run', () {
      expect(
        scheduleRuleText(
          _schedule(rule: 'FREQ=MONTHLY;INTERVAL=1', startDate: '2026-11-01'),
        ),
        'El 1 de cada mes a las 9:00',
      );
    });

    test(
      'a rule that repeats every few periods says so without naming a day',
      () {
        expect(
          scheduleRuleText(_schedule(rule: 'FREQ=WEEKLY;INTERVAL=2;BYDAY=MO')),
          'Repite según su regla a las 9:00',
        );
      },
    );
  });

  group('the words of a schedule and of its runs', () {
    test('the status reads as the person sees it', () {
      expect(scheduleStatusLabel(ScheduleStatus.active), 'Activa');
      expect(scheduleStatusLabel(ScheduleStatus.paused), 'En pausa');
      expect(scheduleStatusLabel(ScheduleStatus.done), 'Hecha');
    });

    test('every run outcome has its own sentence', () {
      expect(runOutcomeLabel(RunOutcome.started), 'Se lanzó');
      expect(
        runOutcomeLabel(RunOutcome.skippedOverlap),
        'Omitida: ya hay una en marcha',
      );
      expect(
        runOutcomeLabel(RunOutcome.skippedCap),
        'Omitida: tope de ejecuciones',
      );
      expect(
        runOutcomeLabel(RunOutcome.missedOffline),
        'Perdida: tu PC estaba apagado',
      );
      expect(runOutcomeLabel(RunOutcome.failed), 'Falló');
    });

    test('«Lanzar ahora» says it launched, or why the brake skipped it', () {
      expect(
        scheduleLaunchText(const ScheduleLaunch.launched()),
        'Encargo lanzado',
      );
      expect(
        scheduleLaunchText(const ScheduleLaunch.skipped(RunOutcome.skippedCap)),
        'Omitida: tope de ejecuciones',
      );
    });

    test('the last run says never when there was none', () {
      expect(scheduleLastText(_schedule().lastRunAt), 'Última: nunca');
    });

    test('the last run shows its day, month and hour', () {
      final last = DateTime(2026, 11, 10, 9, 5);

      expect(scheduleWhenText(last), 'mar 10 nov, 9:05');
      expect(
        scheduleLastText(
          _schedule(lastRunAt: last.toUtc().toIso8601String()).lastRunAt,
        ),
        'Última: mar 10 nov, 9:05',
      );
    });

    test(
      'a paused schedule has no next run; a finished one has none either',
      () {
        expect(
          scheduleNextText(null, ScheduleStatus.paused),
          'Próxima: en pausa',
        );
        expect(scheduleNextText(null, ScheduleStatus.done), 'Próxima: ninguna');
      },
    );

    test('a paused schedule shows no next run even if the host sends one', () {
      final next = DateTime(2026, 11, 12, 9);
      expect(
        scheduleNextText(next, ScheduleStatus.paused),
        'Próxima: en pausa',
      );
      expect(
        scheduleNextText(next, ScheduleStatus.active),
        'Próxima: ${scheduleWhenText(next)}',
      );
    });

    test('the footer keeps the sentence about the PC being on', () {
      expect(
        scheduleFooterText,
        startsWith('Solo se ejecuta con tu PC encendido.'),
      );
    });
  });
}
