import 'package:timezone/data/latest.dart' as tzdata;
import 'package:timezone/timezone.dart' as tz;

/// How many days the calendar shows at once.
enum CalendarRange {
  today(1, 'Hoy'),
  week(7, '7 días'),
  month(30, '30 días');

  const CalendarRange(this.days, this.label);

  final int days;
  final String label;
}

/// The zone called [name], or `null` when this phone's zone data does not know it.
tz.Location? zoneNamed(String name) {
  if (!tz.timeZoneDatabase.isInitialized) tzdata.initializeTimeZones();
  try {
    return tz.getLocation(name);
  } on tz.LocationNotFoundException {
    return null;
  }
}

/// The clock of [zone] at [instant], as a `DateTime` whose fields are the zone's. The
/// value is UTC so that no zone of the phone can skip or repeat one of these hours.
DateTime wallClock(DateTime instant, tz.Location zone) {
  final local = tz.TZDateTime.from(instant, zone);
  return DateTime.utc(
    local.year,
    local.month,
    local.day,
    local.hour,
    local.minute,
  );
}

/// The day of [zone] that holds [instant], at midnight.
DateTime wallDay(DateTime instant, tz.Location zone) =>
    dayOnly(wallClock(instant, zone));

/// Midnight of the day [clock] falls on.
DateTime dayOnly(DateTime clock) =>
    DateTime.utc(clock.year, clock.month, clock.day);

/// [day] moved by [days]. It works on the calendar fields, so no hour is lost.
DateTime addDays(DateTime day, int days) =>
    DateTime.utc(day.year, day.month, day.day + days);

/// [clock] moved by [hours]; the hour may run past midnight.
DateTime addHours(DateTime clock, int hours) => DateTime.utc(
  clock.year,
  clock.month,
  clock.day,
  clock.hour + hours,
  clock.minute,
);

/// The next full hour after [clock], where a new event starts by default.
DateTime nextFullHour(DateTime clock) =>
    DateTime.utc(clock.year, clock.month, clock.day, clock.hour + 1);

/// `YYYY-MM-DD`, the day as the host reads it.
String dayText(DateTime day) =>
    '${_digits(day.year, 4)}-${_digits(day.month, 2)}-${_digits(day.day, 2)}';

/// `YYYY-MM-DDTHH:MM`, the clock as the host reads a timed event.
String clockInput(DateTime clock) =>
    '${dayText(clock)}T${_digits(clock.hour, 2)}:${_digits(clock.minute, 2)}';

/// The days the host is asked for: from [anchor] up to, not including, the day
/// [range] days after it.
({String from, String to}) windowOf(DateTime anchor, CalendarRange range) =>
    (from: dayText(anchor), to: dayText(addDays(anchor, range.days)));

/// The window moved by [steps] of its range: -1 is the one before, 1 the one after.
DateTime shiftAnchor(DateTime anchor, CalendarRange range, int steps) =>
    addDays(anchor, range.days * steps);

String _digits(int value, int width) => value.toString().padLeft(width, '0');
