import '../../core/api/schedule_models.dart';

// The words of «Automatizar». The sentences match the desktop's, so both apps say the same thing.

const String scheduleFooterText =
    'Solo se ejecuta con tu PC encendido. Si estaba apagado a la hora, no se lanza al encenderlo: te avisamos aquí y puedes lanzarla tú.';
const String scheduleEmptyTitle = 'Aún no hay tareas programadas.';
const String scheduleEmptyDetail = 'Programa tareas desde el escritorio.';
const String scheduleLoadingLabel = 'Cargando tareas programadas';
const String scheduleHistoryEmptyText = 'Aún no hay ejecuciones.';

const _months = [
  'ene',
  'feb',
  'mar',
  'abr',
  'may',
  'jun',
  'jul',
  'ago',
  'sept',
  'oct',
  'nov',
  'dic',
];
const _weekdaysShort = ['lun', 'mar', 'mié', 'jue', 'vie', 'sáb', 'dom'];
const _weekdayCodes = ['MO', 'TU', 'WE', 'TH', 'FR', 'SA', 'SU'];
const _weekdayNames = {
  'MO': 'lunes',
  'TU': 'martes',
  'WE': 'miércoles',
  'TH': 'jueves',
  'FR': 'viernes',
  'SA': 'sábado',
  'SU': 'domingo',
};

/// «Cada lunes a las 9:00», «Todos los días a las 18:30», «El 1 de cada mes a las 9:00», or
/// «El 12 de nov. a las 9:00» for a task that runs once.
String scheduleRuleText(Schedule schedule) {
  final clock = _clock(schedule.timeLocal);
  final rule = schedule.rule;
  if (rule == null) return 'El ${_dayMonth(schedule.startDate)} a las $clock';
  final parts = _ruleParts(rule);
  if ((parts['INTERVAL'] ?? '1') != '1') {
    return 'Repite según su regla a las $clock';
  }
  return switch (parts['FREQ']) {
    'DAILY' => 'Todos los días a las $clock',
    'WEEKLY' =>
      'Cada ${_weekdays(parts['BYDAY'], schedule.startDate)} a las $clock',
    'MONTHLY' => 'El ${schedule.startDate.day} de cada mes a las $clock',
    _ => 'Repite según su regla a las $clock',
  };
}

String scheduleStatusLabel(ScheduleStatus status) => switch (status) {
  ScheduleStatus.active => 'Activa',
  ScheduleStatus.paused => 'En pausa',
  ScheduleStatus.done => 'Hecha',
};

String runOutcomeLabel(RunOutcome outcome) => switch (outcome) {
  RunOutcome.started => 'Se lanzó',
  RunOutcome.skippedOverlap => 'Omitida: ya hay una en marcha',
  RunOutcome.skippedCap => 'Omitida: tope de ejecuciones',
  RunOutcome.missedOffline => 'Perdida: tu PC estaba apagado',
  RunOutcome.failed => 'Falló',
};

/// What «Lanzar ahora» said: the launch, or the brake that skipped it.
String scheduleLaunchText(ScheduleLaunch launch) {
  final skipped = launch.skipped;
  return skipped == null ? 'Encargo lanzado' : runOutcomeLabel(skipped);
}

/// «Próxima: mar 10 nov, 9:05», or the word for a task with no next run.
String scheduleNextText(DateTime? next, ScheduleStatus status) {
  if (status == ScheduleStatus.paused) return 'Próxima: en pausa';
  if (next != null) return 'Próxima: ${scheduleWhenText(next)}';
  return 'Próxima: ninguna';
}

/// «Última: mar 10 nov, 9:05», or «Última: nunca» before the first run.
String scheduleLastText(DateTime? last) =>
    last == null ? 'Última: nunca' : 'Última: ${scheduleWhenText(last)}';

/// «mar 10 nov, 9:05»: a day and an hour, in this device's zone.
String scheduleWhenText(DateTime when) =>
    '${_weekdaysShort[when.weekday - 1]} ${when.day} ${_months[when.month - 1]}, '
    '${when.hour}:${when.minute.toString().padLeft(2, '0')}';

String _clock(String hhmm) {
  final parts = hhmm.split(':');
  if (parts.length != 2) return hhmm;
  return '${int.parse(parts[0])}:${parts[1]}';
}

String _dayMonth(DateTime day) => '${day.day} de ${_months[day.month - 1]}.';

String _weekdays(String? byday, DateTime start) {
  final codes = byday == null
      ? [_weekdayCodes[start.weekday - 1]]
      : byday.split(',');
  final names = [for (final code in codes) _weekdayNames[code] ?? code];
  if (names.length == 1) return names.first;
  return '${names.sublist(0, names.length - 1).join(', ')} y ${names.last}';
}

Map<String, String> _ruleParts(String rule) {
  final parts = <String, String>{};
  for (final pair in rule.split(';')) {
    final at = pair.indexOf('=');
    if (at > 0) parts[pair.substring(0, at)] = pair.substring(at + 1);
  }
  return parts;
}
