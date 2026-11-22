import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/agenda_api.dart';
import 'package:excubia/core/state/calendar_time.dart';
import 'package:excubia/ui/agenda/calendar_format.dart';

CalendarEvent _event(
  String id,
  DateTime start, {
  bool allDay = false,
  int? reminder,
}) => CalendarEvent(
  id: id,
  title: 'Evento $id',
  start: start,
  end: null,
  allDay: allDay,
  location: '',
  notes: '',
  reminderMinutes: reminder,
);

String _describe(CalendarLine line) => switch (line) {
  DayLine(:final day) => 'día ${dayText(day)}',
  EventLine(:final event) => event.id,
};

void main() {
  final madrid = zoneNamed('Europe/Madrid')!;
  final today = DateTime.utc(2026, 10, 29);

  group('lista por días', () {
    test('cada día empieza con su encabezado, en la zona de la empresa', () {
      final lines = calendarLines([
        _event('a', DateTime.utc(2026, 10, 29, 8, 30)),
        // 23:30 UTC is 00:30 of the next day in Madrid: a new heading.
        _event('b', DateTime.utc(2026, 10, 29, 23, 30)),
        _event('c', DateTime.utc(2026, 10, 30, 10)),
      ], madrid);

      expect(lines.map(_describe).toList(), [
        'día 2026-10-29',
        'a',
        'día 2026-10-30',
        'b',
        'c',
      ]);
    });

    test('sin eventos no hay líneas', () {
      expect(calendarLines(const [], madrid), isEmpty);
    });
  });

  group('horas y títulos', () {
    test('una hora se muestra en la zona de la empresa', () {
      expect(timeLabel(_event('a', DateTime.utc(2026, 10, 29, 8, 30)), madrid), '09:30');
    });

    test('un evento de todo el día no tiene hora', () {
      expect(
        timeLabel(_event('t', DateTime.utc(2026, 10, 28, 23), allDay: true), madrid),
        'Todo el día',
      );
    });

    test('el reloj tiene dos cifras en horas y minutos', () {
      expect(clockText(DateTime.utc(2026, 10, 29, 9, 5)), '09:05');
    });

    test('el encabezado dice hoy, mañana o el día', () {
      expect(dayHeading(today, today), 'Hoy · jue 29 oct');
      expect(dayHeading(DateTime.utc(2026, 10, 30), today), 'Mañana · vie 30 oct');
      expect(dayHeading(DateTime.utc(2026, 11, 3), today), 'mar 3 nov');
    });

    test('el rango dice el día solo o el primero y el último', () {
      expect(rangeLabel(today, CalendarRange.today), 'jue 29 oct');
      expect(rangeLabel(today, CalendarRange.week), 'jue 29 oct – mié 4 nov');
    });
  });

  group('avisos', () {
    test('«Ninguno» primero, y el aviso del evento aunque no sea preestablecido', () {
      expect(reminderChoices(null), [null, 5, 15, 30, 60, 1440]);
      expect(reminderChoices(10), [null, 5, 10, 15, 30, 60, 1440]);
      expect(reminderChoices(15), [null, 5, 15, 30, 60, 1440]);
    });

    test('cada aviso se dice en español', () {
      expect(reminderLabel(0), 'Al empezar');
      expect(reminderLabel(5), '5 min antes');
      expect(reminderLabel(90), '90 min antes');
      expect(reminderLabel(60), '1 h antes');
      expect(reminderLabel(120), '2 h antes');
      expect(reminderLabel(1440), '1 día antes');
      expect(reminderLabel(2880), '2 días antes');
    });
  });
}
