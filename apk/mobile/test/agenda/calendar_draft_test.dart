import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/agenda_api.dart';
import 'package:excubia/core/state/calendar_time.dart';
import 'package:excubia/ui/agenda/calendar_draft.dart';
import 'package:excubia/ui/agenda/calendar_recurrence.dart';

CalendarDraft _timed({
  String title = 'Llamada',
  String location = '',
  String notes = '',
  DateTime? end,
}) => CalendarDraft(
  title: title,
  allDay: false,
  start: DateTime.utc(2026, 10, 29, 10),
  end: end ?? DateTime.utc(2026, 10, 29, 11),
  location: location,
  notes: notes,
);

CalendarDraft _allDay({required DateTime start, required DateTime end}) =>
    CalendarDraft(title: 'Viaje', allDay: true, start: start, end: end);

void main() {
  final madrid = zoneNamed('Europe/Madrid')!;

  group('lo que se puede guardar', () {
    test('un evento nuevo sin título no pasa', () {
      expect(
        CalendarDraft.fresh(start: DateTime.utc(2026, 10, 29, 10)).problem(),
        'El título es obligatorio.',
      );
    });

    test('un evento con título y hora pasa, con o sin fin', () {
      expect(_timed().problem(), isNull);
      expect(
        CalendarDraft(
          title: 'Llamada',
          allDay: false,
          start: DateTime.utc(2026, 10, 29, 10),
        ).problem(),
        isNull,
      );
    });

    test('un fin que no es posterior al inicio no pasa', () {
      expect(
        _timed(end: DateTime.utc(2026, 10, 29, 10)).problem(),
        'El fin debe ser posterior al inicio.',
      );
    });

    test('en un todo el día el último día no puede ser anterior al primero', () {
      expect(
        _allDay(start: DateTime.utc(2026, 10, 30), end: DateTime.utc(2026, 10, 29))
            .problem(),
        'El último día no puede ser anterior al primero.',
      );
      expect(
        _allDay(start: DateTime.utc(2026, 10, 30), end: DateTime.utc(2026, 10, 30))
            .problem(),
        isNull,
      );
    });
  });

  group('límites', () {
    test('el título cuenta caracteres, hasta 120', () {
      expect(_timed(title: 'a' * 120).problem(), isNull);
      expect(
        _timed(title: 'a' * 121).problem(),
        'El título tiene 120 caracteres como máximo.',
      );
    });

    test('el lugar cuenta caracteres, hasta 200', () {
      expect(_timed(location: 'a' * 200).problem(), isNull);
      expect(
        _timed(location: 'a' * 201).problem(),
        'El lugar tiene 200 caracteres como máximo.',
      );
    });

    test('las notas cuentan bytes, hasta 4 KB', () {
      expect(_timed(notes: 'a' * 4096).problem(), isNull);
      expect(_timed(notes: 'a' * 4097).problem(), 'Las notas pasan de 4 KB.');
      // Each «ñ» takes two bytes: 2048 of them are exactly 4 KB.
      expect(_timed(notes: 'ñ' * 2048).problem(), isNull);
      expect(_timed(notes: 'ñ' * 2049).problem(), 'Las notas pasan de 4 KB.');
    });
  });

  group('lo que se envía', () {
    test('un evento con hora manda reloj del día, sin lo vacío', () {
      expect(
        _timed().toInput().toCreateJson(),
        {
          'title': 'Llamada',
          'all_day': false,
          'start': '2026-10-29T10:00',
          'end': '2026-10-29T11:00',
        },
      );
    });

    test('un todo el día manda solo los días', () {
      expect(
        _allDay(start: DateTime.utc(2026, 10, 29, 10), end: DateTime.utc(2026, 10, 31))
            .toInput()
            .toCreateJson(),
        {
          'title': 'Viaje',
          'all_day': true,
          'start': '2026-10-29',
          'end': '2026-10-31',
        },
      );
    });
  });

  group('desde un evento guardado', () {
    test('un evento con hora se lee en la hora de la empresa', () {
      final draft = CalendarDraft.of(
        CalendarEvent(
          id: 'ev_1',
          title: 'Llamada',
          start: DateTime.utc(2026, 10, 29, 8, 30),
          end: DateTime.utc(2026, 10, 29, 9, 30),
          allDay: false,
          location: 'Oficina',
          notes: '',
          reminderMinutes: 15,
        ),
        madrid,
      );

      expect(draft.start, DateTime.utc(2026, 10, 29, 9, 30));
      expect(draft.end, DateTime.utc(2026, 10, 29, 10, 30));
      expect(draft.location, 'Oficina');
      expect(draft.reminderMinutes, 15);
    });

    test('el fin de un todo el día es el último día, no la medianoche siguiente', () {
      final draft = CalendarDraft.of(
        CalendarEvent(
          id: 'ev_2',
          title: 'Viaje',
          // Local midnight of 29 October and of 31 October (Madrid is UTC+1).
          start: DateTime.utc(2026, 10, 28, 23),
          end: DateTime.utc(2026, 10, 30, 23),
          allDay: true,
          location: '',
          notes: '',
          reminderMinutes: null,
        ),
        madrid,
      );

      expect(draft.start, DateTime.utc(2026, 10, 29));
      expect(draft.end, DateTime.utc(2026, 10, 30));
    });
  });

  group('repetición', () {
    CalendarEvent series(String? recurrence) => CalendarEvent.fromJson({
      'id': 'ev_1',
      'title': 'Equipo',
      'start_utc': '2026-10-29T08:30:00.000Z',
      'end_utc': null,
      'all_day': false,
      'recurrence': ?recurrence,
      'location': '',
      'notes': '',
      'reminder_minutes': null,
    });

    test('una serie se lee con su regla y se vuelve a enviar igual', () {
      final draft = CalendarDraft.of(
        series('FREQ=WEEKLY;INTERVAL=1;BYDAY=TH;COUNT=10'),
        madrid,
      );

      expect(draft.repeat, isA<RepeatRule>());
      expect(draft.toInput().recurrence, 'FREQ=WEEKLY;INTERVAL=1;BYDAY=TH;COUNT=10');
    });

    test('una regla que el formulario no escribe no se reenvía', () {
      final draft = CalendarDraft.of(
        series('FREQ=MONTHLY;INTERVAL=1;BYMONTHDAY=31'),
        madrid,
      );

      expect(draft.repeat, isA<CustomRepeat>());
      expect(draft.toInput().recurrence, isNull);
    });

    test('un evento sin regla se queda sin repetir', () {
      final draft = CalendarDraft.of(series(null), madrid);

      expect(draft.repeat, isA<NoRepeat>());
      expect(draft.toInput().recurrence, '');
    });

    test('una repetición mal armada impide guardar', () {
      final draft = CalendarDraft(
        title: 'Equipo',
        allDay: false,
        start: DateTime.utc(2026, 10, 29, 10),
        end: DateTime.utc(2026, 10, 29, 11),
        repeat: RepeatRule.starting(RepeatFreq.daily, DateTime.utc(2026, 10, 29))
            .copyWith(interval: 0),
      );

      expect(draft.problem(), 'Cada cuántos: entre 1 y 999.');
    });
  });
}
