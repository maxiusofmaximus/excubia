import '../../core/api/agenda_api.dart';

/// A moment in Spanish, relative to [now]: «ahora», «hace 5 min», «hace 3 h»,
/// «hoy», «ayer», «hace 4 días», and past that the day and month.
String relativeDate(DateTime when, DateTime now) {
  final local = when.toLocal();
  final elapsed = now.difference(local);
  if (elapsed < const Duration(minutes: 1)) return 'ahora';
  if (elapsed < const Duration(hours: 1)) {
    return 'hace ${elapsed.inMinutes} min';
  }
  final days = DateTime(
    now.year,
    now.month,
    now.day,
  ).difference(DateTime(local.year, local.month, local.day)).inDays;
  if (days == 0) return 'hace ${elapsed.inHours} h';
  if (days == 1) return 'ayer';
  if (days < 7) return 'hace $days días';
  return '${_two(local.day)}/${_two(local.month)}/${local.year}';
}

String _two(int value) => value.toString().padLeft(2, '0');

/// The row title of a note: its subject, else the start of its text.
String noteTitle(AgendaNoteRow row) {
  final subject = row.subject?.trim() ?? '';
  if (subject.isNotEmpty) return subject;
  final excerpt = row.excerpt.trim();
  return excerpt.isEmpty ? 'Nota sin texto' : excerpt;
}

/// The lines of a multi-line field, trimmed, without the empty ones.
List<String> splitLines(String text) => [
  for (final line in text.split('\n'))
    if (line.trim().isNotEmpty) line.trim(),
];

/// The totals of an import in one line, with no name and no address.
String importSummary(AgendaImportReport report) =>
    '${report.created} nuevos, ${report.merged} ampliados, '
    '${report.skipped} omitidos, ${report.rejected} rechazados';
