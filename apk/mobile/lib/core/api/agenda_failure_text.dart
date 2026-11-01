part of 'agenda_api.dart';

/// Spanish text for a failed agenda call. The host's own words are not shown;
/// the one name it writes, the owner of an address, is what the person needs.
String agendaFailureText(Object error) {
  if (error is! GatewayError) return _unknownFailure;
  final detail = _detailOf(error.body);
  final failure = error.failure;
  if (failure != null && detail.code == null) return connectionText(failure);
  return switch (error.status) {
    0 => 'No se pudo contactar con el equipo.',
    403 => 'Falta un permiso de este dispositivo para esta acción. Pide a quien administra el equipo que lo conceda.',
    404 => 'Ya no existe. Vuelve a la lista.',
    413 => 'El archivo pesa más de 512 KiB.',
    429 => 'Demasiadas acciones seguidas. Espera un momento.',
    _ => _codeText(detail),
  };
}

const String _unknownFailure =
    'El equipo no pudo completar la acción. Inténtalo de nuevo.';

String _codeText(
  ({String? code, String? field, String? message}) detail,
) => switch (detail.code) {
  'address_taken' => _addressTakenText(detail.message),
  'alias_taken' => 'Ese alias ya lo usa otro contacto.',
  'invalid_address' => 'Una dirección no tiene la forma correo@dominio.com.',
  'invalid_arguments' || 'invalid_field' => _invalidFieldText(detail.field),
  'empty_message' => 'La nota está vacía. Escribe algo antes de guardar.',
  'invalid_body' => 'El texto de la nota no se pudo guardar (máximo 32 KB).',
  'unknown_message' => 'Ese mensaje ya no existe en el chat.',
  'note_limit' => 'Ya hay 2000 notas. Borra alguna para crear otra.',
  'contact_limit' => 'Ya hay 1000 contactos. Borra alguno para crear otro.',
  'not_found' => 'Ya no existe. Vuelve a la lista.',
  'timezone_unset' => 'Antes hay que fijar la zona horaria de la empresa.',
  'timezone_already_set' => 'La zona horaria de la empresa ya está fijada y no cambia.',
  'invalid_zone' => _zoneText,
  'invalid_local_time' =>
    'Esa hora no existe en la zona de la empresa: el cambio de horario la salta.',
  'calendar_limit' => 'Ya hay 5000 eventos. Borra alguno para crear otro.',
  'unsupported_recurrence' => 'Esa repetición todavía no está soportada.',
  'invalid_recurrence' => _recurrenceText,
  'invalid_ics' => 'El archivo no es un calendario .ics válido.',
  _ => _unknownFailure,
};

const String _zoneText = 'Esa zona no existe. Usa un nombre como Europe/Madrid.';

const String _recurrenceText = 'La repetición no es válida.';

/// shortcut: the owner's name is read from the host's English message ("...belongs
/// to the contact NAME"), upgrade when the host sends it as its own field.
String _addressTakenText(String? message) {
  final name = RegExp(r'belongs to the contact (.+)$')
      .firstMatch(message ?? '');
  if (name == null) return 'Esa dirección ya es de otro contacto.';
  return 'Esa dirección ya es de «${name.group(1)}».';
}

String _invalidFieldText(String? field) => switch (field) {
  'addresses' => 'Hay más de 10 direcciones, o alguna pasa de 320 caracteres.',
  'aliases' => 'Hay más de 10 alias, o alguno es demasiado largo.',
  'name' => 'El nombre es obligatorio y tiene 120 caracteres como máximo.',
  'subject' =>
    'El asunto va en una sola línea, con 120 caracteres como máximo.',
  'title' => 'El título es obligatorio y tiene 120 caracteres como máximo.',
  'location' => 'El lugar tiene 200 caracteres como máximo.',
  'notes' => 'Las notas son demasiado largas (máximo 4 KB).',
  'start' => 'La fecha o la hora de inicio no son válidas.',
  'end' => 'El fin debe ser posterior al inicio.',
  'timezone' => _zoneText,
  'reminder_minutes' => 'El aviso no es válido.',
  'recurrence' => _recurrenceText,
  _ => 'Revisa los datos e inténtalo de nuevo.',
};

/// The `detail` of a host error: its stable code, the field to fix and the message.
({String? code, String? field, String? message}) _detailOf(String body) {
  final decoded = _decode(body);
  final detail = decoded is Map<String, Object?> ? decoded['detail'] : null;
  if (detail is! Map<String, Object?>) {
    return (code: null, field: null, message: null);
  }
  return (
    code: jsonOptionalText(detail, 'code'),
    field: jsonOptionalText(detail, 'field'),
    message: jsonOptionalText(detail, 'message'),
  );
}

Object? _decode(String body) {
  try {
    return jsonDecode(body);
  } on FormatException {
    return null;
  }
}
