import '../../core/state/calendar_time.dart';
import 'calendar_format.dart';

/// What «Repetir» says about an event. [CustomRepeat] is a rule the form cannot write
/// back: it is shown as it is, and sent as it is unless the person changes it.
sealed class Repeat {
  const Repeat();

  /// The rule to send: `''` takes the rule away, `null` leaves the stored one alone.
  String? get wire;

  /// The first thing wrong with this repeat, in Spanish, or `null` when it can go.
  String? problem(DateTime startDay) => null;
}

final class NoRepeat extends Repeat {
  const NoRepeat();

  @override
  String get wire => '';
}

final class CustomRepeat extends Repeat {
  const CustomRepeat(this.rule);

  /// The rule as the host keeps it.
  final String rule;

  @override
  String? get wire => null;
}

final class RepeatRule extends Repeat {
  const RepeatRule({
    required this.freq,
    this.interval = 1,
    this.days = const <int>{},
    this.end = const EndsNever(),
  });

  /// A new rule that starts on [startDay]: every one, on that weekday, forever.
  factory RepeatRule.starting(RepeatFreq freq, DateTime startDay) =>
      RepeatRule(freq: freq, days: {startDay.weekday});

  final RepeatFreq freq;
  final int interval;

  /// The weekdays of a weekly rule, as `DateTime.monday` to `DateTime.sunday`.
  final Set<int> days;
  final RepeatEnd end;

  RepeatRule copyWith({
    RepeatFreq? freq,
    int? interval,
    Set<int>? days,
    RepeatEnd? end,
  }) => RepeatRule(
    freq: freq ?? this.freq,
    interval: interval ?? this.interval,
    days: days ?? this.days,
    end: end ?? this.end,
  );

  @override
  String get wire => [
    'FREQ=${freq.wire}',
    'INTERVAL=$interval',
    if (freq == RepeatFreq.weekly) 'BYDAY=${_dayCodesOf(days)}',
    ?end.wire,
  ].join(';');

  @override
  String? problem(DateTime startDay) {
    if (interval < 1 || interval > 999) return 'Cada cuántos: entre 1 y 999.';
    if (freq == RepeatFreq.weekly && days.isEmpty) {
      return 'Elige al menos un día de la semana.';
    }
    return end.problem(startDay);
  }
}

/// When a rule stops: never, after some occurrences, or on a day (the last one).
sealed class RepeatEnd {
  const RepeatEnd();

  String? get wire;

  String? problem(DateTime startDay) => null;
}

final class EndsNever extends RepeatEnd {
  const EndsNever();

  @override
  String? get wire => null;
}

final class EndsAfter extends RepeatEnd {
  const EndsAfter(this.count);

  final int count;

  @override
  String get wire => 'COUNT=$count';

  @override
  String? problem(DateTime startDay) =>
      count < 1 || count > 1000 ? 'Las veces: entre 1 y 1000.' : null;
}

final class EndsOn extends RepeatEnd {
  const EndsOn(this.day);

  /// The last day of the rule, as the company's day (midnight, UTC).
  final DateTime day;

  @override
  String get wire => 'UNTIL=${dayText(day).replaceAll('-', '')}';

  @override
  String? problem(DateTime startDay) => day.isBefore(startDay)
      ? 'La fecha de fin no puede ser anterior al inicio.'
      : null;
}

/// The frequencies «Repetir» offers, with the words of «Cada …».
enum RepeatFreq {
  daily('DAILY', 'día', 'días'),
  weekly('WEEKLY', 'semana', 'semanas'),
  monthly('MONTHLY', 'mes', 'meses'),
  yearly('YEARLY', 'año', 'años');

  const RepeatFreq(this.wire, this.unitOne, this.unitMany);

  final String wire;
  final String unitOne;
  final String unitMany;

  static RepeatFreq? fromWire(String? wire) {
    for (final freq in values) {
      if (freq.wire == wire) return freq;
    }
    return null;
  }

  String unit(int interval) => interval == 1 ? unitOne : unitMany;

  /// «Cada día», «Cada 3 meses».
  String words(int interval) =>
      interval == 1 ? 'Cada $unitOne' : 'Cada $interval $unitMany';
}

/// The repeat a stored rule is, or a [CustomRepeat] when the form cannot write it back.
Repeat readRepeat(String? wire) {
  if (wire == null || wire.isEmpty) return const NoRepeat();
  return _readRule(wire) ?? CustomRepeat(wire);
}

/// The rule in words: «Cada semana, lunes y miércoles, 10 veces».
String repeatWords(Repeat repeat) => switch (repeat) {
  NoRepeat() => 'No se repite',
  CustomRepeat() => 'Repetición personalizada',
  RepeatRule rule => [
    rule.freq.words(rule.interval),
    if (rule.freq == RepeatFreq.weekly && rule.days.isNotEmpty)
      _daysWords(rule.days),
    ?_endWords(rule.end),
  ].join(', '),
};

/// The name of a weekday, as `DateTime.monday` … `DateTime.sunday`.
String weekdayName(int weekday) => _weekdayNames[weekday - 1];

const List<String> _weekdayNames = [
  'lunes',
  'martes',
  'miércoles',
  'jueves',
  'viernes',
  'sábado',
  'domingo',
];

const List<String> _dayCodes = ['MO', 'TU', 'WE', 'TH', 'FR', 'SA', 'SU'];

const Set<String> _knownKeys = {'FREQ', 'INTERVAL', 'BYDAY', 'COUNT', 'UNTIL'};

RepeatRule? _readRule(String wire) {
  final parts = <String, String>{};
  for (final part in wire.split(';')) {
    final at = part.indexOf('=');
    if (at < 0) return null;
    final key = part.substring(0, at);
    if (!_knownKeys.contains(key) || parts.containsKey(key)) return null;
    parts[key] = part.substring(at + 1);
  }
  final freq = RepeatFreq.fromWire(parts['FREQ']);
  final interval = int.tryParse(parts['INTERVAL'] ?? '1');
  final days = _readDays(parts['BYDAY'], freq);
  final end = _readEnd(parts);
  if (freq == null || interval == null || days == null || end == null) {
    return null;
  }
  return RepeatRule(freq: freq, interval: interval, days: days, end: end);
}

/// The weekdays of a rule; `null` when the form cannot show them. A weekly rule
/// without `BYDAY` depends on its start, so it is not read as a list of days.
Set<int>? _readDays(String? byday, RepeatFreq? freq) {
  if (freq != RepeatFreq.weekly) return byday == null ? <int>{} : null;
  if (byday == null) return null;
  final days = <int>{};
  for (final code in byday.split(',')) {
    final index = _dayCodes.indexOf(code);
    if (index < 0) return null;
    days.add(index + 1);
  }
  return days;
}

RepeatEnd? _readEnd(Map<String, String> parts) {
  final count = parts['COUNT'];
  final until = parts['UNTIL'];
  if (count != null && until != null) return null;
  if (count != null) {
    final times = int.tryParse(count);
    return times == null ? null : EndsAfter(times);
  }
  if (until == null) return const EndsNever();
  final day = _readDay(until);
  return day == null ? null : EndsOn(day);
}

/// `YYYYMMDD` as a day. An instant (`YYYYMMDDTHHMMSSZ`) is not a day the form can show.
DateTime? _readDay(String text) {
  final match = RegExp(r'^(\d{4})(\d{2})(\d{2})$').firstMatch(text);
  if (match == null) return null;
  return DateTime.utc(
    int.parse(match[1]!),
    int.parse(match[2]!),
    int.parse(match[3]!),
  );
}

String _dayCodesOf(Set<int> days) {
  final sorted = days.toList()..sort();
  return sorted.map((day) => _dayCodes[day - 1]).join(',');
}

/// «lunes», «lunes y miércoles», «lunes, miércoles y viernes».
String _daysWords(Set<int> days) {
  final names = [for (final day in days.toList()..sort()) weekdayName(day)];
  if (names.length == 1) return names.single;
  return '${names.sublist(0, names.length - 1).join(', ')} y ${names.last}';
}

String? _endWords(RepeatEnd end) => switch (end) {
  EndsNever() => null,
  EndsAfter(:final count) => count == 1 ? '1 vez' : '$count veces',
  EndsOn(:final day) => 'hasta el ${dayLabel(day)} ${day.year}',
};
