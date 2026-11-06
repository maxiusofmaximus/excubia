part of 'schedules_api.dart';

/// Said where the device may only see the schedules: it lacks `APPROVE`.
const String schedulePermissionText =
    'Este dispositivo solo puede ver las tareas programadas. Concede el permiso APPROVE desde el PC para pausarlas o lanzarlas.';

const String _unknownRead =
    'No se pudieron leer las tareas programadas. Inténtalo de nuevo.';
const String _unknownAction =
    'No se pudo completar la acción. Inténtalo de nuevo.';

/// The wait for the grant, or null for every other failure of a read.
String? scheduleReadWaitingText(Object error) =>
    isReadForbidden(error) ? readGrantWaitingText : null;

/// Spanish text for a schedule list that could not be read. A link failure keeps its own words.
String scheduleReadFailureText(Object error) {
  final waiting = scheduleReadWaitingText(error);
  if (waiting != null) return waiting;
  final failure = error is HostError ? error.failure : null;
  return failure == null ? _unknownRead : connectionText(failure);
}

/// Spanish text for a refused pause, resume or launch. The host's stable code is worded here.
String scheduleActionFailureText(Object error) {
  if (error is! HostError) return _unknownAction;
  final code = jsonErrorCode(error.body);
  final failure = error.failure;
  if (failure != null && code == null) return connectionText(failure);
  return switch (code) {
    'no_such_schedule' => 'Esa tarea ya no existe. Se quitará de la lista.',
    'schedule_done' => 'Esta tarea ya está hecha: no se puede cambiar.',
    'schedule_no_future_run' =>
      'Ya no quedan ejecuciones futuras. Programa una nueva desde el PC.',
    _ => _statusText(error.status),
  };
}

String _statusText(int status) => switch (status) {
  403 => schedulePermissionText,
  404 => 'Esa tarea ya no existe. Se quitará de la lista.',
  _ => _unknownAction,
};
