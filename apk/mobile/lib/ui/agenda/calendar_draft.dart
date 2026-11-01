import 'dart:convert';

import 'package:timezone/timezone.dart' as tz;

import '../../core/api/agenda_api.dart';
import '../../core/state/calendar_time.dart';
import 'calendar_recurrence.dart';

const int calendarTitleMax = 120;
const int calendarLocationMax = 200;
const int calendarNotesMaxBytes = 4096;

/// The fields of the editor as they will be sent. The start and the end are clock
/// readings of the company's zone. An all-day event uses only their days, and its
/// [end] is the last day it covers.
class CalendarDraft {
  const CalendarDraft({
    required this.title,
    required this.allDay,
    required this.start,
    this.end,
    this.location = '',
    this.notes = '',
    this.reminderMinutes,
    this.repeat = const NoRepeat(),
  });

  /// A new timed event, an hour long, from [start].
  factory CalendarDraft.fresh({required DateTime start}) => CalendarDraft(
    title: '',
    allDay: false,
    start: start,
    end: addHours(start, 1),
  );

  /// The fields of a stored event, read in [zone].
  factory CalendarDraft.of(CalendarEvent event, tz.Location zone) {
    final end = event.end;
    return CalendarDraft(
      title: event.title,
      allDay: event.allDay,
      start: event.allDay
          ? wallDay(event.start, zone)
          : wallClock(event.start, zone),
      // An all-day end is the midnight after its last day, so the last day is the one before.
      end: switch (end) {
        null => null,
        DateTime last when event.allDay => addDays(wallDay(last, zone), -1),
        DateTime last => wallClock(last, zone),
      },
      location: event.location,
      notes: event.notes,
      reminderMinutes: event.reminderMinutes,
      repeat: readRepeat(event.recurrence),
    );
  }

  final String title;
  final bool allDay;
  final DateTime start;
  final DateTime? end;
  final String location;
  final String notes;
  final int? reminderMinutes;
  final Repeat repeat;

  /// The first thing wrong with the fields, in Spanish, or `null` when they can go.
  String? problem() {
    if (title.isEmpty) return 'El título es obligatorio.';
    if (title.runes.length > calendarTitleMax) {
      return 'El título tiene $calendarTitleMax caracteres como máximo.';
    }
    if (location.runes.length > calendarLocationMax) {
      return 'El lugar tiene $calendarLocationMax caracteres como máximo.';
    }
    if (utf8.encode(notes).length > calendarNotesMaxBytes) {
      return 'Las notas pasan de 4 KB.';
    }
    return _windowProblem() ?? repeat.problem(dayOnly(start));
  }

  String? _windowProblem() {
    final last = end;
    if (last == null) return null;
    if (allDay) {
      return dayOnly(last).isBefore(dayOnly(start))
          ? 'El último día no puede ser anterior al primero.'
          : null;
    }
    return last.isAfter(start) ? null : 'El fin debe ser posterior al inicio.';
  }

  /// The body for the host. Days and clock times are the company's.
  CalendarEventInput toInput() {
    final last = end;
    return CalendarEventInput(
      title: title,
      allDay: allDay,
      start: _when(start),
      end: last == null ? null : _when(last),
      location: location,
      notes: notes,
      reminderMinutes: reminderMinutes,
      recurrence: repeat.wire,
    );
  }

  String _when(DateTime clock) => allDay ? dayText(clock) : clockInput(clock);
}
