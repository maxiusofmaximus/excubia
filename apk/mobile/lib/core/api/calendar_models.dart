part of 'agenda_api.dart';

/// An event of the company's calendar. [start] and [end] are instants in UTC. An
/// all-day event starts at local midnight and ends at the midnight after its last
/// day; [end] is `null` when a timed event has none.
class CalendarEvent {
  const CalendarEvent({
    required this.id,
    required this.title,
    required this.start,
    required this.end,
    required this.allDay,
    required this.location,
    required this.notes,
    required this.reminderMinutes,
    this.recurrence,
    this.occurrenceStart,
    this.recurring = false,
  });

  factory CalendarEvent.fromJson(Map<String, Object?> json) => CalendarEvent(
    id: jsonText(json, 'id'),
    title: jsonText(json, 'title'),
    start: _time(json, 'start_utc'),
    end: _optionalTime(json, 'end_utc'),
    allDay: json['all_day'] == true,
    location: jsonOptionalText(json, 'location') ?? '',
    notes: jsonOptionalText(json, 'notes') ?? '',
    reminderMinutes: _optionalInt(json, 'reminder_minutes'),
    recurrence: jsonOptionalText(json, 'recurrence'),
    occurrenceStart: _optionalTime(json, 'occurrence_start'),
    recurring: json['recurring'] == true,
  );

  final String id;
  final String title;
  final DateTime start;
  final DateTime? end;
  final bool allDay;
  final String location;
  final String notes;

  /// Minutes before [start]; `null` when the event has no reminder.
  final int? reminderMinutes;

  /// The repeat rule of the series, as the host keeps it; `null` when it happens once.
  final String? recurrence;

  /// The occurrence a list entry stands for. Its [start] is the same instant; a single
  /// read has none.
  final DateTime? occurrenceStart;

  /// Whether a list entry is one occurrence of a series.
  final bool recurring;
}

/// What the editor sends. The times are clock readings of the company's zone: a day
/// (`YYYY-MM-DD`) for an all-day event, where [end] is the last day it covers, or a
/// clock time (`YYYY-MM-DDTHH:MM`).
class CalendarEventInput {
  const CalendarEventInput({
    required this.title,
    required this.allDay,
    required this.start,
    this.end,
    this.location = '',
    this.notes = '',
    this.reminderMinutes,
    this.recurrence,
  });

  final String title;
  final bool allDay;
  final String start;
  final String? end;
  final String location;
  final String notes;

  /// `null` means no reminder.
  final int? reminderMinutes;

  /// The repeat rule. `''` takes the rule away on an update; `null` leaves the stored
  /// rule alone, which is how a rule the form cannot write is kept.
  final String? recurrence;

  /// The body of a create: what is empty is left out.
  Map<String, Object?> toCreateJson() => {
    'title': title,
    'all_day': allDay,
    'start': start,
    'end': ?end,
    'location': ?(location.isEmpty ? null : location),
    'notes': ?(notes.isEmpty ? null : notes),
    'reminder_minutes': ?reminderMinutes,
    'recurrence': ?(recurrence == '' ? null : recurrence),
  };

  /// The body of an update: what is empty is sent empty, which takes the end, the
  /// location or the notes away. No reminder is `-1`.
  Map<String, Object?> toUpdateJson() => {
    'title': title,
    'all_day': allDay,
    'start': start,
    'end': end ?? '',
    'location': location,
    'notes': notes,
    'reminder_minutes': reminderMinutes ?? -1,
    'recurrence': ?recurrence,
  };
}

DateTime? _optionalTime(Map<String, Object?> json, String key) {
  final value = jsonOptionalText(json, key);
  return value == null ? null : DateTime.parse(value);
}

int? _optionalInt(Map<String, Object?> json, String key) {
  final value = json[key];
  return value is int ? value : null;
}
