part of 'restore_api.dart';

/// The grant the restore needs, said the way the desktop says it.
const String restoreGrantLede =
    'Para volver a una copia, este dispositivo necesita el permiso APPROVE. Concédelo desde el PC.';

const String _unknownFailure =
    'El equipo no pudo completar la acción. Inténtalo de nuevo.';

/// A read the host refused with 403 is a device still waiting for its READ_STATE grant.
/// Null for every other failure, which [restoreFailureText] words.
String? restoreWaitingText(Object error) =>
    isReadForbidden(error) ? readGrantWaitingText : null;

/// Spanish text for a failed restore, read or undo. The host's words are not shown;
/// its stable code is worded in the desktop's own sentences.
String restoreFailureText(Object error) {
  if (error is! HostError) return _unknownFailure;
  final code = jsonErrorCode(error.body);
  final failure = error.failure;
  if (failure != null && code == null) return connectionText(failure);
  return switch (code) {
    'restore_point_not_found' => 'Esa copia ya no existe. Vuelve a la lista.',
    'restore_invalid' => 'La copia tiene rutas que salen de la carpeta, así que no se ha restaurado nada.',
    'restore_failed' =>
      'No se pudo volver a esa copia. Revisa la carpeta y reinténtalo.',
    _ => _statusText(error.status),
  };
}

String _statusText(int status) => switch (status) {
  403 => restoreGrantLede,
  404 => 'Ya no existe. Vuelve a la lista.',
  _ => _unknownFailure,
};
