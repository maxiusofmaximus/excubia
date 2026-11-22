import 'dart:convert';

// The readers the host's answers go through. A missing or mistyped required field throws,
// so a broken answer is never shown as an empty one; only the optional readers return null
// or an empty value.

/// The host's `{ok: true, ...}` envelope; any other answer is broken.
Map<String, Object?> jsonEnvelope(String body) {
  final decoded = jsonDecode(body);
  if (decoded is! Map<String, Object?> || decoded['ok'] != true) {
    throw const FormatException('respuesta inesperada');
  }
  return decoded;
}

/// A value that must be an object: a list item or a required field.
Map<String, Object?> jsonObject(Object? value) {
  if (value is Map<String, Object?>) return value;
  throw const FormatException('respuesta inesperada');
}

/// An optional object: anything else reads as an empty one.
Map<String, Object?> jsonObjectOrEmpty(Object? value) =>
    value is Map<String, Object?> ? value : const {};

List<Object?> jsonList(Map<String, Object?> json, String key) {
  final value = json[key];
  if (value is List<Object?>) return value;
  throw FormatException('falta $key');
}

String jsonText(Map<String, Object?> json, String key) {
  final value = json[key];
  if (value is String) return value;
  throw FormatException('falta $key');
}

String? jsonOptionalText(Map<String, Object?> json, String key) {
  final value = json[key];
  return value is String ? value : null;
}

/// Only a JSON integer: a double such as `1.0` is broken, as it always was here.
int jsonInt(Map<String, Object?> json, String key) {
  final value = json[key];
  if (value is int) return value;
  throw FormatException('falta $key');
}

/// The strings of a list; an item of any other type is skipped, not an error.
List<String> jsonStrings(Map<String, Object?> json, String key) => [
  for (final item in jsonList(json, key))
    if (item is String) item,
];

/// The `detail.code` of a coded refusal; a bare `detail` text has none.
String? jsonErrorCode(String body) {
  Object? decoded;
  try {
    decoded = jsonDecode(body);
  } on FormatException {
    return null;
  }
  final detail = decoded is Map<String, Object?> ? decoded['detail'] : null;
  return detail is Map<String, Object?> ? jsonOptionalText(detail, 'code') : null;
}
