import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/restore_models.dart';

Map<String, Object?> _point({
  String id = '01A',
  String kind = 'turn',
  int createdMs = 1000,
  int files = 42,
  int excluded = 3,
}) => {
  'id': id,
  'kind': kind,
  'turn': '01T',
  'created_ms': createdMs,
  'files': files,
  'excluded': excluded,
};

void main() {
  group('RestorePoint.fromJson', () {
    test('reads the copy the host listed', () {
      final point = RestorePoint.fromJson(_point());

      expect(point.id, '01A');
      expect(point.kind, RestorePointKind.turn);
      expect(point.createdMs, 1000);
      expect(point.files, 42);
      expect(point.excluded, 3);
    });

    test('reads a copy taken before an undo', () {
      final point = RestorePoint.fromJson(_point(kind: 'before_undo'));

      expect(point.kind, RestorePointKind.beforeUndo);
    });

    test('refuses a kind it does not know instead of showing it', () {
      expect(
        () => RestorePoint.fromJson(_point(kind: 'otra')),
        throwsFormatException,
      );
    });

    test('refuses a missing count instead of showing zero files', () {
      final json = _point()..remove('files');

      expect(() => RestorePoint.fromJson(json), throwsFormatException);
    });
  });

  group('sortRestorePoints', () {
    test('puts the newest first and leaves the list it was given', () {
      final given = [
        RestorePoint.fromJson(_point(id: 'old', createdMs: 1)),
        RestorePoint.fromJson(_point(id: 'new', createdMs: 9)),
        RestorePoint.fromJson(_point(id: 'mid', createdMs: 5)),
      ];

      final sorted = sortRestorePoints(given);

      expect(sorted.map((point) => point.id), ['new', 'mid', 'old']);
      expect(given.map((point) => point.id), ['old', 'new', 'mid']);
    });
  });

  group('changes', () {
    test('maps the three statuses the host sends', () {
      expect(changeStatusOf('added'), ChangeStatus.added);
      expect(changeStatusOf('modified'), ChangeStatus.modified);
      expect(changeStatusOf('deleted'), ChangeStatus.deleted);
    });

    test('refuses a status it does not know', () {
      expect(() => changeStatusOf('movido'), throwsFormatException);
    });

    test('reads the changed and the left-out files of a copy', () {
      final changes = RestoreChanges.fromJson({
        'ok': true,
        'point': '01A',
        'changes': [
          {'path': 'src/a.md', 'status': 'added'},
          {'path': 'src/b.md', 'status': 'deleted'},
        ],
        'excluded': [
          {'path': 'node_modules', 'reason': 'too_large'},
        ],
      });

      expect(changes.changes.length, 2);
      expect(changes.changes.first.path, 'src/a.md');
      expect(changes.changes.first.status, ChangeStatus.added);
      expect(changes.excluded.single.reason, ExcludedReason.tooLarge);
    });
  });

  group('excludedReasonOf', () {
    test('maps each reason the host names', () {
      expect(excludedReasonOf('name'), ExcludedReason.name);
      expect(excludedReasonOf('too_large'), ExcludedReason.tooLarge);
      expect(excludedReasonOf('symlink'), ExcludedReason.symlink);
      expect(excludedReasonOf('special'), ExcludedReason.special);
      expect(excludedReasonOf('unsupported_name'),
          ExcludedReason.unsupportedName);
    });

    test('reads a reason from a newer host as other, not as an error', () {
      expect(excludedReasonOf('nuevo_motivo'), ExcludedReason.other);
    });
  });

  test('reads what a restore put back, took away and kept as undo', () {
    final result = RestoreResult.fromJson({
      'ok': true,
      'point': '01A',
      'restored': 2,
      'removed': 1,
      'undo_point': '01U',
    });

    expect(result.restored, 2);
    expect(result.removed, 1);
    expect(result.undoPoint, '01U');
  });
}
