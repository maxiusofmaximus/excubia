part of 'team_summary_api.dart';

const String _unknownFailure = 'No se pudo leer tu equipo. Inténtalo de nuevo.';

/// The wait for the grant, or null for every other failure.
String? teamSummaryWaitingText(Object error) =>
    isReadForbidden(error) ? readGrantWaitingText : null;

/// Said once when the team cannot be read. A link failure keeps its own words.
String teamSummaryFailureText(Object error) {
  final waiting = teamSummaryWaitingText(error);
  if (waiting != null) return waiting;
  final failure = error is HostError ? error.failure : null;
  return failure == null ? _unknownFailure : connectionText(failure);
}
