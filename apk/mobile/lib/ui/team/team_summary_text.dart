import '../../core/api/team_summary_models.dart';

// The words of «Tu equipo», the same sentences as the desktop band.

String teamStatusLabel(TeamStatus status) => switch (status) {
  TeamStatus.waitingForYou => 'Espera tu visto bueno',
  TeamStatus.blocked => 'Necesita ayuda',
  TeamStatus.working => 'Trabajando',
  TeamStatus.done => 'Terminó',
  TeamStatus.idle => 'Sin actividad',
};

/// «1 espera tu visto bueno», «2 esperan tu visto bueno»: read out when it changes.
String teamWaitingText(int count) {
  if (count == 0) return 'Nadie espera tu visto bueno.';
  return count == 1
      ? '1 espera tu visto bueno'
      : '$count esperan tu visto bueno';
}

/// «desde hace 12 min». Null when the host sent no time. A clock that runs behind
/// the host's reads as a moment, never as a negative age.
String? teamAgeLabel(DateTime? since, DateTime now) {
  if (since == null) return null;
  final elapsed = now.toUtc().difference(since.toUtc());
  return 'desde hace ${_span(elapsed)}';
}

String _span(Duration elapsed) {
  final minutes = elapsed.inMinutes;
  if (minutes < 1) return 'un momento';
  if (minutes < 60) return '$minutes min';
  final hours = elapsed.inHours;
  if (hours < 24) return '$hours h';
  final days = elapsed.inDays;
  return days == 1 ? '1 día' : '$days días';
}
