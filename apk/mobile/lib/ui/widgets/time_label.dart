/// A compact, local-time label for chat metadata. The contract sends RFC3339
/// UTC; the screen never shows the raw timestamp.
String relativeTime(String? iso, {DateTime? now}) {
  final parsed = iso == null ? null : DateTime.tryParse(iso);
  if (parsed == null) return 'sin actividad';
  final local = parsed.toLocal();
  final today = now ?? DateTime.now();
  final sameDay =
      local.year == today.year &&
      local.month == today.month &&
      local.day == today.day;
  if (sameDay) return _clock(local);
  final yesterday = today.subtract(const Duration(days: 1));
  if (local.year == yesterday.year &&
      local.month == yesterday.month &&
      local.day == yesterday.day) {
    return 'ayer';
  }
  if (local.year == today.year) {
    return '${local.day.toString().padLeft(2, '0')}/'
        '${local.month.toString().padLeft(2, '0')}';
  }
  return '${local.day.toString().padLeft(2, '0')}/'
      '${local.month.toString().padLeft(2, '0')}/${local.year}';
}

String _clock(DateTime value) =>
    '${value.hour.toString().padLeft(2, '0')}:'
    '${value.minute.toString().padLeft(2, '0')}';
