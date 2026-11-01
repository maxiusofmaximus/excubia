part of 'agenda_api.dart';

/// Spanish text for a failed `.ics` call. The event limit is said in the words of the
/// direction that hit it: an import is split, an export is narrowed.
String icsFailureText(Object error, {required bool importing}) {
  if (_codeOf(error) != 'too_many_events') return agendaFailureText(error);
  return importing
      ? 'Tiene más de 500 eventos: divide el archivo.'
      : 'Hay demasiados eventos: elige un rango más corto.';
}

/// Whether the company has no zone yet, so the calendar must ask for one.
bool icsZoneUnset(Object error) => _codeOf(error) == 'timezone_unset';

String? _codeOf(Object error) =>
    error is GatewayError ? _detailOf(error.body).code : null;
