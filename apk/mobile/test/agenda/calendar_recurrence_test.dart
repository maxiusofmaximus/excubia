import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/ui/agenda/calendar_recurrence.dart';

final DateTime _start = DateTime.utc(2026, 10, 29);

void main() {
  group('leer una regla existente', () {
    test('sin regla no se repite', () {
      expect(readRepeat(null), isA<NoRepeat>());
      expect(readRepeat(''), isA<NoRepeat>());
    });

    test('una semanal con días y veces se lee entera', () {
      final rule = readRepeat(
        'FREQ=WEEKLY;INTERVAL=1;BYDAY=MO,WE;COUNT=10',
      ) as RepeatRule;

      expect(rule.freq, RepeatFreq.weekly);
      expect(rule.interval, 1);
      expect(rule.days, {DateTime.monday, DateTime.wednesday});
      expect((rule.end as EndsAfter).count, 10);
    });

    test('una fecha de fin se lee como día', () {
      final rule =
          readRepeat('FREQ=DAILY;INTERVAL=2;UNTIL=20261103') as RepeatRule;

      expect(rule.freq, RepeatFreq.daily);
      expect(rule.interval, 2);
      expect((rule.end as EndsOn).day, DateTime.utc(2026, 11, 3));
    });

    test('sin fin se lee como nunca', () {
      final rule = readRepeat('FREQ=YEARLY;INTERVAL=1') as RepeatRule;

      expect(rule.end, isA<EndsNever>());
      expect(rule.days, isEmpty);
    });

    test('lo que el formulario no sabe escribir se conserva tal cual', () {
      for (final wire in [
        'FREQ=MONTHLY;INTERVAL=1;BYMONTHDAY=31',
        'FREQ=WEEKLY;INTERVAL=1',
        'FREQ=DAILY;INTERVAL=1;UNTIL=20261103T000000Z',
      ]) {
        final repeat = readRepeat(wire);

        expect(repeat, isA<CustomRepeat>(), reason: wire);
        expect(repeat.wire, isNull, reason: wire);
      }
    });
  });

  group('escribir la regla', () {
    test('sin repetición se quita; la personalizada no se toca', () {
      expect(const NoRepeat().wire, '');
      expect(const CustomRepeat('FREQ=YEARLY').wire, isNull);
    });

    test('una semanal nueva parte del día de inicio', () {
      expect(
        RepeatRule.starting(RepeatFreq.weekly, _start).wire,
        'FREQ=WEEKLY;INTERVAL=1;BYDAY=TH',
      );
    });

    test('se escribe en el orden del host: FREQ, INTERVAL, BYDAY y fin', () {
      final rule = RepeatRule(
        freq: RepeatFreq.weekly,
        interval: 2,
        days: {DateTime.wednesday, DateTime.monday},
        end: EndsAfter(10),
      );

      expect(rule.wire, 'FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;COUNT=10');
    });

    test('los días solo van en las semanales', () {
      final rule = RepeatRule(
        freq: RepeatFreq.daily,
        days: {DateTime.monday},
        end: EndsOn(DateTime.utc(2026, 11, 3)),
      );

      expect(rule.wire, 'FREQ=DAILY;INTERVAL=1;UNTIL=20261103');
    });

    test('lo que se lee se vuelve a escribir igual', () {
      for (final wire in [
        'FREQ=WEEKLY;INTERVAL=1;BYDAY=MO,WE;COUNT=10',
        'FREQ=DAILY;INTERVAL=2;UNTIL=20261103',
        'FREQ=YEARLY;INTERVAL=1',
      ]) {
        expect(readRepeat(wire).wire, wire);
      }
    });
  });

  group('lo que el formulario no deja guardar', () {
    RepeatRule daily() => RepeatRule.starting(RepeatFreq.daily, _start);

    test('cada cuántos fuera de 1 a 999', () {
      const message = 'Cada cuántos: entre 1 y 999.';

      expect(daily().copyWith(interval: 0).problem(_start), message);
      expect(daily().copyWith(interval: 1000).problem(_start), message);
    });

    test('una semanal sin días no pasa', () {
      final weekly = RepeatRule.starting(RepeatFreq.weekly, _start);

      expect(
        weekly.copyWith(days: {}).problem(_start),
        'Elige al menos un día de la semana.',
      );
    });

    test('las veces fuera de 1 a 1000', () {
      const message = 'Las veces: entre 1 y 1000.';

      expect(daily().copyWith(end: EndsAfter(0)).problem(_start), message);
      expect(daily().copyWith(end: EndsAfter(1001)).problem(_start), message);
    });

    test('una fecha de fin anterior al inicio no pasa', () {
      expect(
        daily()
            .copyWith(end: EndsOn(DateTime.utc(2026, 10, 28)))
            .problem(_start),
        'La fecha de fin no puede ser anterior al inicio.',
      );
    });

    test(
      'una regla bien armada, la ausencia de regla y la personalizada pasan',
      () {
        expect(daily().problem(_start), isNull);
        expect(const NoRepeat().problem(_start), isNull);
        expect(const CustomRepeat('FREQ=YEARLY').problem(_start), isNull);
      },
    );
  });

  group('la regla en palabras', () {
    test('sin repetición', () {
      expect(repeatWords(const NoRepeat()), 'No se repite');
    });

    test('semanal con días y veces', () {
      final rule = RepeatRule(
        freq: RepeatFreq.weekly,
        days: {DateTime.monday, DateTime.wednesday},
        end: EndsAfter(10),
      );

      expect(repeatWords(rule), 'Cada semana, lunes y miércoles, 10 veces');
    });

    test('diaria sin fin', () {
      expect(
        repeatWords(RepeatRule.starting(RepeatFreq.daily, _start)),
        'Cada día',
      );
    });

    test('cada varios meses y años', () {
      expect(
        repeatWords(
          RepeatRule.starting(RepeatFreq.monthly, _start).copyWith(interval: 3),
        ),
        'Cada 3 meses',
      );
      expect(
        repeatWords(
          RepeatRule.starting(RepeatFreq.yearly, _start).copyWith(interval: 2),
        ),
        'Cada 2 años',
      );
    });

    test('hasta una fecha la nombra', () {
      final rule = RepeatRule.starting(
        RepeatFreq.daily,
        _start,
      ).copyWith(end: EndsOn(DateTime.utc(2026, 11, 3)));

      expect(repeatWords(rule), 'Cada día, hasta el mar 3 nov 2026');
    });
  });
}
