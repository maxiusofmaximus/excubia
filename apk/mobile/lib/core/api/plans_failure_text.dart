part of 'plans_api.dart';

/// Said where the device may only read the plans: it lacks `APPROVE`.
const String planPermissionText =
    'Este dispositivo solo puede ver los planes. Concede el permiso APPROVE desde el PC para decidir.';

const String _unknownDecision = 'No se pudo completar la acción. Inténtalo de nuevo.';
const String _unknownRead = 'No se pudo leer los planes. Inténtalo de nuevo.';

/// The wait for the grant, or null for every other failure of a read.
String? planReadWaitingText(Object error) =>
    isReadForbidden(error) ? readGrantWaitingText : null;

/// Spanish text for a plan that could not be read. A link failure keeps its own words.
String planReadFailureText(Object error) {
  final waiting = planReadWaitingText(error);
  if (waiting != null) return waiting;
  final failure = error is HostError ? error.failure : null;
  return failure == null ? _unknownRead : connectionText(failure);
}

/// Spanish text for a refused decision. The host's stable code is worded in the desktop's
/// own sentences; anything else says the action did not finish.
String planDecisionFailureText(Object error) {
  if (error is! HostError) return _unknownDecision;
  final code = jsonErrorCode(error.body);
  final failure = error.failure;
  if (failure != null && code == null) return connectionText(failure);
  return switch (code) {
    'plan_decided' => 'Este plan ya se decidió desde otro dispositivo.',
    'plan_not_approved' => 'Este plan todavía no está aprobado.',
    'no_such_plan' => 'Ese plan ya no existe. Se quitará de la lista.',
    'invalid_body' => 'La nota no es válida. Revísala y vuelve a enviarla.',
    _ => _statusText(error.status),
  };
}

String _statusText(int status) => switch (status) {
  403 => planPermissionText,
  404 => 'Ese plan ya no existe. Se quitará de la lista.',
  _ => _unknownDecision,
};
