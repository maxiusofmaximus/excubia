import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/restore_models.dart';
import 'package:excubia/ui/restore/restore_text.dart';

const int _minute = 60000;
const int _hour = 60 * _minute;
const int _day = 24 * _hour;
const int _now = 10 * _day;

RestoreExcluded _excluded(ExcludedReason reason) =>
    RestoreExcluded(path: 'x', reason: reason);

void main() {
  group('pointAge', () {
    test('says a moment for less than a minute', () {
      expect(pointAge(_now - 20000, _now), 'hace un momento');
    });

    test('counts minutes, then hours', () {
      expect(pointAge(_now - 5 * _minute, _now), 'hace 5 min');
      expect(pointAge(_now - 3 * _hour, _now), 'hace 3 h');
    });

    test('says a day in the singular and counts the rest in the plural', () {
      expect(pointAge(_now - _day, _now), 'hace 1 día');
      expect(pointAge(_now - 2 * _day, _now), 'hace 2 días');
    });
  });

  test('pointTime is the local clock as HH:MM', () {
    final created = DateTime(2026, 11, 5, 9, 4).millisecondsSinceEpoch;

    expect(pointTime(created), '09:04');
  });

  test('pointLabel tells a turn copy from one taken before an undo', () {
    final turn = RestorePoint.fromJson(_turnJson(RestorePointKind.turn));
    final undo = RestorePoint.fromJson(_turnJson(RestorePointKind.beforeUndo));

    expect(pointLabel(turn), 'Copia del turno');
    expect(pointLabel(undo), 'Copia antes de deshacer');
  });

  test('filesText is singular for one file', () {
    expect(filesText(1), '1 fichero');
    expect(filesText(0), '0 ficheros');
    expect(filesText(42), '42 ficheros');
  });

  test('changeLabel says new, changed and deleted in Spanish', () {
    expect(changeLabel(ChangeStatus.added), 'nuevo');
    expect(changeLabel(ChangeStatus.modified), 'cambiado');
    expect(changeLabel(ChangeStatus.deleted), 'borrado');
  });

  group('excludedSentence', () {
    test('is null when nothing was left out', () {
      expect(excludedSentence(const []), isNull);
    });

    test('names one left-out element in the singular', () {
      expect(
        excludedSentence([_excluded(ExcludedReason.tooLarge)]),
        'No se guardó 1 elemento: archivos o carpetas muy grandes.',
      );
    });

    test('counts many and names each reason once, in order', () {
      expect(
        excludedSentence([
          _excluded(ExcludedReason.symlink),
          _excluded(ExcludedReason.special),
          _excluded(ExcludedReason.symlink),
        ]),
        'No se guardaron 3 elementos: enlaces y elementos especiales.',
      );
    });

    test('joins three reasons with commas and y', () {
      expect(
        excludedSentence([
          _excluded(ExcludedReason.name),
          _excluded(ExcludedReason.tooLarge),
          _excluded(ExcludedReason.unsupportedName),
        ]),
        'No se guardaron 3 elementos: claves o carpetas técnicas, '
        'archivos o carpetas muy grandes y nombres que no se pueden guardar.',
      );
    });
  });

  test('confirmText names what goes back and keeps a copy to undo', () {
    expect(
      confirmText(1),
      'Se va a restaurar 1 fichero. Antes se guarda una copia de la carpeta actual para poder deshacer.',
    );
    expect(
      confirmText(3),
      'Se van a restaurar 3 ficheros. Antes se guarda una copia de la carpeta actual para poder deshacer.',
    );
  });

  test('doneText says what came back and what was taken away', () {
    expect(
      doneText(
        RestoreResult.fromJson(_result(restored: 2, removed: 0)),
      ),
      'Se han restaurado 2 ficheros.',
    );
    expect(
      doneText(RestoreResult.fromJson(_result(restored: 2, removed: 1))),
      'Se han restaurado 2 ficheros. Se han quitado 1 fichero.',
    );
  });

  test('keeps the warning the desktop shows, word for word', () {
    expect(
      restoreWarning,
      'Se compara la carpeta antes y después; los cambios hechos por comandos no se siguen uno a uno.',
    );
  });
}

Map<String, Object?> _turnJson(RestorePointKind kind) => {
  'id': '01A',
  'kind': kind == RestorePointKind.turn ? 'turn' : 'before_undo',
  'created_ms': 1000,
  'files': 1,
  'excluded': 0,
};

Map<String, Object?> _result({required int restored, required int removed}) => {
  'point': '01A',
  'restored': restored,
  'removed': removed,
  'undo_point': '01U',
};
