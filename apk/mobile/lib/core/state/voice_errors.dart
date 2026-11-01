import '../api/connection_text.dart';
import '../api/gateway_client.dart';

/// What the user reads when a voice note is refused. Codes come from the
/// host's voice contract; a connection failure says why the PC is unreachable.
String voiceErrorText(Object error) {
  if (error is! GatewayError) return _generic;
  final body = error.body;
  if (body.contains('"voice_unavailable"')) {
    return 'El PC no tiene la voz instalada o no está lista.';
  }
  if (body.contains('"voice_busy"')) return voiceBusyText;
  if (error.status == 413 || body.contains('"audio_too_long"')) {
    return 'La nota es demasiado larga.';
  }
  if (body.contains('"invalid_audio"')) {
    return 'No se pudo leer la nota. Vuelve a grabarla.';
  }
  final failure = error.failure;
  if (failure != null) return connectionText(failure);
  if (error.status == 0) return 'Sin conexión con el PC.';
  return _generic;
}

const voiceBusyText =
    'El PC está transcribiendo otra nota. Prueba en unos segundos.';

const voicePermissionText =
    'Sin permiso de micrófono. Concédelo en los ajustes de la app para dictar.';

const voiceStartFailedText = 'No se pudo empezar a grabar.';

const voiceEmptyText = 'No se oyó nada';

const _generic = 'No se pudo transcribir la nota. Vuelve a intentarlo.';
