part of 'mail_api.dart';

/// Spanish text for a failed send. It names no code and no server text.
String mailSendFailureText(MailSendFailure failure) => switch (failure.code) {
  MailFailureCode.accountSuspended => 'Microsoft suspendió temporalmente el envío de esta cuenta. Entra en Outlook.com para verificarla y vuelve a intentarlo más tarde.',
  MailFailureCode.authExpired => 'La sesión de la cuenta de correo caducó. Vuelve a conectar la cuenta en Ajustes › Correo.',
  MailFailureCode.throttled => 'Microsoft pidió esperar antes de enviar más correos. No se ha enviado nada; inténtalo en unos minutos.',
  MailFailureCode.graphUnavailable when failure.safe =>
    'No se pudo contactar con Microsoft. No se ha enviado nada.',
  MailFailureCode.badRequest =>
    'Microsoft rechazó el borrador. Revisa destinatarios y asunto.',
  MailFailureCode.draftChanged => 'El borrador cambió después de aprobarlo. No se ha enviado nada; pide el envío otra vez.',
  MailFailureCode.graphUnavailable ||
  MailFailureCode.interrupted ||
  MailFailureCode.unknown => 'El envío no se completó y puede que el correo haya salido. Revisa Enviados antes de volver a intentarlo.',
};

/// Spanish text for a refused or failed mail call. The mapping is closed: codes,
/// addresses, paths and tokens never reach the screen.
String mailFailureText(Object error) {
  if (error is! GatewayError) return _unknownFailure;
  final code = _codeOf(error.body);
  final failure = error.failure;
  if (failure != null && code == null) return connectionText(failure);
  if (error.status == 0) {
    return 'No se pudo contactar con el equipo.';
  }
  // A host can answer "not configured" with a 403 too: the code says which.
  if (error.status == 403 && code != 'mail_not_configured') {
    return 'Este teléfono no tiene permiso para gestionar el correo. '
        'Pide a quien administra el equipo que te lo conceda.';
  }
  if (code == null && error.status == 429) {
    return _rateLimited(error.retryAfter);
  }
  return mailCodeText(code, retryAfter: error.retryAfter);
}

/// Spanish text for one stable host code. Unknown codes get the generic text.
String mailCodeText(String? code, {int? retryAfter}) => switch (code) {
  'mail_not_configured' => 'El correo aún no está configurado en tu equipo. Falta registrar la aplicación de Microsoft; quien administra el equipo puede hacerlo.',
  'mail_invalid_input' =>
    'Algún dato no es válido. Revísalo e inténtalo de nuevo.',
  'mail_unavailable' => 'Microsoft no responde ahora. Inténtalo más tarde.',
  'mail_auth_failed' ||
  'mail_auth_required' => 'Hay que volver a conectar esta cuenta.',
  'mail_login_expired' => 'El código caducó. Empieza de nuevo.',
  'mail_login_denied' => 'No se completó el inicio de sesión.',
  'mail_admin_consent_required' =>
    'El administrador de la organización tiene que aprobar el acceso.',
  'mail_app_not_registered' => 'La app de correo no está registrada en Microsoft. Revisa la configuración del equipo.',
  'mail_send_not_enabled' =>
    'Esta cuenta no tiene permitido enviar. Activa «Permitir enviar» primero.',
  'mail_approval_stale' => 'El borrador cambió. Revísalo de nuevo.',
  'mail_approval_denied' => 'El envío no se aprobó.',
  'mail_draft_refused' =>
    'Rechazaste este borrador. Cámbialo para pedir el envío otra vez.',
  'mail_unknown_draft' =>
    'Ese borrador no lo creó este equipo. Revisa el id e inténtalo de nuevo.',
  'mail_send_rate_limited' => _rateLimited(retryAfter),
  _ => _unknownFailure,
};

const String _unknownFailure =
    'El equipo no pudo completar la acción de correo. Inténtalo de nuevo.';

String _rateLimited(int? seconds) {
  final wait = seconds == null
      ? 'un rato'
      : seconds < 60
      ? '$seconds s'
      : '${(seconds / 60).ceil()} min';
  return 'Has enviado muchos correos seguidos. Inténtalo de nuevo en $wait.';
}

/// The stable code of a refusal. The mail routes answer `{"detail": {"code"}}`;
/// the action errors of the chat answer `{"code"}` at the top level.
String? _codeOf(String body) {
  try {
    final decoded = jsonDecode(body);
    if (decoded is! Map<String, Object?>) return null;
    final detail = decoded['detail'];
    final holder = detail is Map<String, Object?> ? detail : decoded;
    final code = holder['code'];
    return code is String ? code : null;
  } on FormatException {
    return null;
  }
}
