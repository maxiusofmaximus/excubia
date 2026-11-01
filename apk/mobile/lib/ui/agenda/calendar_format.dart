import 'package:timezone/timezone.dart' as tz;

import '../../core/api/agenda_api.dart';
import '../../core/state/calendar_time.dart';

/// One line of the calendar list: the heading of a day, or one of its events.
sealed class CalendarLine {
  const CalendarLine();
}

final class DayLine extends CalendarLine {
  const DayLine(this.day);

  final DateTime day;
}

final class EventLine extends CalendarLine {
  const EventLine(this.event);

  final CalendarEvent event;
}

/// The events, in the order the host sends them (by start), with the heading of each
/// day before its first event. The days are those of [zone].
List<CalendarLine> calendarLines(List<CalendarEvent> events, tz.Location zone) {
  final lines = <CalendarLine>[];
  DateTime? shown;
  for (final event in events) {
    final day = wallDay(event.start, zone);
    if (day != shown) {
      lines.add(DayLine(day));
      shown = day;
    }
    lines.add(EventLine(event));
  }
  return lines;
}

/// The start of [event] on the company's clock: «09:30», or «Todo el día».
String timeLabel(CalendarEvent event, tz.Location zone) =>
    event.allDay ? 'Todo el día' : clockText(wallClock(event.start, zone));

/// `HH:MM`.
String clockText(DateTime clock) =>
    '${_two(clock.hour)}:${_two(clock.minute)}';

/// «Hoy · lun 3 nov», «Mañana · mar 4 nov», or the day alone.
String dayHeading(DateTime day, DateTime today) {
  final prefix = switch (day.difference(today).inDays) {
    0 => 'Hoy · ',
    1 => 'Mañana · ',
    _ => '',
  };
  return '$prefix${dayLabel(day)}';
}

/// A day as «lun 3 nov».
String dayLabel(DateTime day) =>
    '${_weekdays[day.weekday - 1]} ${day.day} ${_months[day.month - 1]}';

/// The days on screen, as «lun 3 nov – dom 9 nov», or just the day for «Hoy».
String rangeLabel(DateTime anchor, CalendarRange range) => range.days == 1
    ? dayLabel(anchor)
    : '${dayLabel(anchor)} – ${dayLabel(addDays(anchor, range.days - 1))}';

/// The reminders the editor offers, in minutes before the start.
const List<int> calendarReminderPresets = [5, 15, 30, 60, 1440];

/// «Ninguno» first, then the presets, and the event's own reminder when it is not one
/// of them: the host accepts any number of minutes.
List<int?> reminderChoices(int? current) {
  final minutes = {...calendarReminderPresets, ?current}.toList()..sort();
  return [null, ...minutes];
}

/// A reminder in words: «Al empezar», «15 min antes», «2 h antes», «1 día antes».
String reminderLabel(int minutes) {
  if (minutes == 0) return 'Al empezar';
  if (minutes % 1440 == 0) {
    return minutes == 1440 ? '1 día antes' : '${minutes ~/ 1440} días antes';
  }
  if (minutes % 60 == 0) return '${minutes ~/ 60} h antes';
  return '$minutes min antes';
}

const List<String> _weekdays = ['lun', 'mar', 'mié', 'jue', 'vie', 'sáb', 'dom'];

const List<String> _months = [
  'ene', 'feb', 'mar', 'abr', 'may', 'jun',
  'jul', 'ago', 'sep', 'oct', 'nov', 'dic',
];

String _two(int value) => value.toString().padLeft(2, '0');
