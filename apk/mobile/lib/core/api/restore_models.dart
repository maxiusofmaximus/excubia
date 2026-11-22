import 'json_read.dart';

// The copies of the folder («Copias del turno»). The host keeps them; a device reads
// them with READ_STATE and restores with APPROVE (contract `api.md`, F2).

/// A turn copy is taken before each turn; a before-undo copy keeps the folder as it
/// was just before a restore, so the restore can be taken back.
enum RestorePointKind { turn, beforeUndo }

/// Why a file of a copy was left out. A reason from a newer host reads as [other].
enum ExcludedReason { name, tooLarge, symlink, special, unsupportedName, other }

enum ChangeStatus { added, modified, deleted }

ExcludedReason excludedReasonOf(String wire) => switch (wire) {
  'name' => ExcludedReason.name,
  'too_large' => ExcludedReason.tooLarge,
  'symlink' => ExcludedReason.symlink,
  'special' => ExcludedReason.special,
  'unsupported_name' => ExcludedReason.unsupportedName,
  _ => ExcludedReason.other,
};

/// A status the host does not name is a broken answer, so it is refused.
ChangeStatus changeStatusOf(String wire) => switch (wire) {
  'added' => ChangeStatus.added,
  'modified' => ChangeStatus.modified,
  'deleted' => ChangeStatus.deleted,
  _ => throw FormatException('cambio desconocido: $wire'),
};

RestorePointKind _kindOf(String wire) => switch (wire) {
  'turn' => RestorePointKind.turn,
  'before_undo' => RestorePointKind.beforeUndo,
  _ => throw FormatException('copia desconocida: $wire'),
};

/// One copy in the list. Its files are counted, not named: the names come with [RestoreChanges].
class RestorePoint {
  const RestorePoint({
    required this.id,
    required this.kind,
    required this.createdMs,
    required this.files,
    required this.excluded,
  });

  factory RestorePoint.fromJson(Map<String, Object?> json) => RestorePoint(
    id: jsonText(json, 'id'),
    kind: _kindOf(jsonText(json, 'kind')),
    createdMs: jsonInt(json, 'created_ms'),
    files: jsonInt(json, 'files'),
    excluded: jsonInt(json, 'excluded'),
  );

  final String id;
  final RestorePointKind kind;
  final int createdMs;
  final int files;
  final int excluded;
}

/// Newest first. The list it is given is left as it came.
List<RestorePoint> sortRestorePoints(Iterable<RestorePoint> points) =>
    List<RestorePoint>.of(points)
      ..sort((a, b) => b.createdMs.compareTo(a.createdMs));

class RestoreChange {
  const RestoreChange({required this.path, required this.status});

  factory RestoreChange.fromJson(Map<String, Object?> json) => RestoreChange(
    path: jsonText(json, 'path'),
    status: changeStatusOf(jsonText(json, 'status')),
  );

  final String path;
  final ChangeStatus status;
}

class RestoreExcluded {
  const RestoreExcluded({required this.path, required this.reason});

  factory RestoreExcluded.fromJson(Map<String, Object?> json) =>
      RestoreExcluded(
        path: jsonText(json, 'path'),
        reason: excludedReasonOf(jsonText(json, 'reason')),
      );

  final String path;
  final ExcludedReason reason;
}

/// What a copy would change in the folder, and what it did not keep.
class RestoreChanges {
  const RestoreChanges({required this.changes, required this.excluded});

  factory RestoreChanges.fromJson(Map<String, Object?> json) => RestoreChanges(
    changes: [
      for (final item in jsonList(json, 'changes'))
        RestoreChange.fromJson(jsonObject(item)),
    ],
    excluded: [
      for (final item in jsonList(json, 'excluded'))
        RestoreExcluded.fromJson(jsonObject(item)),
    ],
  );

  final List<RestoreChange> changes;
  final List<RestoreExcluded> excluded;
}

/// What a restore put back and took away. [undoPoint] is the copy that takes it back.
class RestoreResult {
  const RestoreResult({
    required this.point,
    required this.restored,
    required this.removed,
    required this.undoPoint,
  });

  factory RestoreResult.fromJson(Map<String, Object?> json) => RestoreResult(
    point: jsonText(json, 'point'),
    restored: jsonInt(json, 'restored'),
    removed: jsonInt(json, 'removed'),
    undoPoint: jsonText(json, 'undo_point'),
  );

  final String point;
  final int restored;
  final int removed;
  final String undoPoint;
}
