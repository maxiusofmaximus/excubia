import '../../core/api/restore_models.dart';

// The words of «Copias del turno». The desktop says the same things in the same
// order; keep the two in step when one changes.

/// Shown under the confirmation, before anything is put back.
const String restoreWarning =
    'Se compara la carpeta antes y después; los cambios hechos por comandos no se siguen uno a uno.';

/// «hace un momento», «hace 5 min», «hace 3 h», «hace 1 día», «hace 2 días».
String pointAge(int createdMs, int nowMs) {
  final minutes = (nowMs - createdMs) ~/ 60000;
  if (minutes < 1) return 'hace un momento';
  if (minutes < 60) return 'hace $minutes min';
  final hours = minutes ~/ 60;
  if (hours < 24) return 'hace $hours h';
  final days = hours ~/ 24;
  return days == 1 ? 'hace 1 día' : 'hace $days días';
}

/// The local clock of the copy, as `HH:MM`.
String pointTime(int createdMs) {
  final date = DateTime.fromMillisecondsSinceEpoch(createdMs);
  return '${date.hour.toString().padLeft(2, '0')}:'
      '${date.minute.toString().padLeft(2, '0')}';
}

String pointLabel(RestorePoint point) => switch (point.kind) {
  RestorePointKind.turn => 'Copia del turno',
  RestorePointKind.beforeUndo => 'Copia antes de deshacer',
};

String filesText(int count) => count == 1 ? '1 fichero' : '$count ficheros';

String changeLabel(ChangeStatus status) => switch (status) {
  ChangeStatus.added => 'nuevo',
  ChangeStatus.modified => 'cambiado',
  ChangeStatus.deleted => 'borrado',
};

String _excludedLabel(ExcludedReason reason) => switch (reason) {
  ExcludedReason.name => 'claves o carpetas técnicas',
  ExcludedReason.tooLarge => 'archivos o carpetas muy grandes',
  ExcludedReason.symlink => 'enlaces',
  ExcludedReason.special => 'elementos especiales',
  ExcludedReason.unsupportedName => 'nombres que no se pueden guardar',
  ExcludedReason.other => 'otros elementos',
};

/// «No se guardaron 3 elementos: a, b y c.» Null when nothing was left out. Each
/// reason is named once, in the order the host sent it.
String? excludedSentence(List<RestoreExcluded> excluded) {
  if (excluded.isEmpty) return null;
  final reasons = {
    for (final item in excluded) _excludedLabel(item.reason),
  }.toList();
  final count = excluded.length;
  final head = count == 1
      ? 'No se guardó 1 elemento'
      : 'No se guardaron $count elementos';
  return '$head: ${joinLabels(reasons)}.';
}

/// What the confirmation names before anything is put back.
String confirmText(int count) {
  final what = count == 1
      ? 'Se va a restaurar 1 fichero.'
      : 'Se van a restaurar $count ficheros.';
  return '$what Antes se guarda una copia de la carpeta actual para poder deshacer.';
}

/// What a finished restore put back and what it took away.
String doneText(RestoreResult result) {
  final back = 'Se han restaurado ${filesText(result.restored)}.';
  return result.removed == 0
      ? back
      : '$back Se han quitado ${filesText(result.removed)}.';
}

/// Joins labels the way a Spanish sentence does: «a», «a y b», «a, b y c».
String joinLabels(List<String> labels) {
  if (labels.length <= 1) return labels.join();
  return '${labels.sublist(0, labels.length - 1).join(', ')} y ${labels.last}';
}
