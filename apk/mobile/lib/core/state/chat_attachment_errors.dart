import '../../src/rust/api/error.dart' show FailureKind;
import '../../src/rust/api/error/chat.dart';
import '../api/host_client.dart';

/// What the user reads for an attachment refusal. Codes the host does not name
/// fall back to the generic line.
String attachmentErrorText(String code) => switch (code) {
  'unsupported_type' => 'Solo PNG, JPEG o WebP',
  'image_too_large' ||
  'dimensions_too_large' => 'La imagen es demasiado grande',
  'conversation_quota' =>
    'Se llenó el espacio de adjuntos de esta conversación',
  'host_quota' => 'Se llenó el espacio de adjuntos del equipo',
  'timeout' ||
  'worker_failed' => 'No se pudo procesar la imagen. Prueba con otra',
  'unknown_attachment' => 'Vuelve a adjuntar la imagen',
  'too_many_attachments' => 'Solo puedes adjuntar hasta 4 imágenes',
  'unreadable' => 'No se pudo leer la imagen',
  _ => 'No se pudo subir la imagen. Vuelve a intentarlo',
};

/// The send-side failure of a refused attachment, shaped like the chat's other
/// refusals so the screen shows it the same way.
ChatFailure attachmentChatProblem(String code) => ChatFailure(
  code: code,
  message: attachmentErrorText(code),
  kind: FailureKind.retry,
  shownInTimeline: false,
  keptByHost: false,
);

/// The host refused a message because a named file is unknown, not ready or
/// already sent. The Rust mapping reads 422 by field, so the code is found in
/// the body.
bool namesUnknownAttachment(Object error) =>
    error is HostError && error.body.contains('"unknown_attachment"');
