import 'dart:convert';

import 'agenda_api.dart';
import 'gateway_client.dart';

/// Spanish text for a failed rule call. The two rule codes have their own words; every other
/// failure reads as the agenda's, since the host's routes answer the same way.
String alertsFailureText(Object error) => switch (_codeOf(error)) {
  'rule_exists' => 'Ya existe una regla igual.',
  'rule_limit' => 'Has llegado al máximo de 100 reglas.',
  _ => agendaFailureText(error),
};

/// The stable code of a host error, or null when the body has none. The alert routes answer
/// `{"detail": "rule_exists"}`; the agenda's answer `{"detail": {"code": …}}`.
String? _codeOf(Object error) {
  if (error is! GatewayError) return null;
  final Object? decoded;
  try {
    decoded = jsonDecode(error.body);
  } on FormatException {
    return null;
  }
  final detail = decoded is Map<String, Object?> ? decoded['detail'] : null;
  if (detail is String) return detail;
  final code = detail is Map<String, Object?> ? detail['code'] : null;
  return code is String ? code : null;
}
