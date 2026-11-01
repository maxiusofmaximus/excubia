import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/agenda_api.dart';
import 'package:herdr_notify/ui/agenda/agenda_format.dart';

void main() {
  final now = DateTime(2026, 10, 29, 12, 0);

  group('relativeDate', () {
    test('reads a recent moment in minutes and hours', () {
      expect(relativeDate(now.subtract(const Duration(seconds: 20)), now), 'ahora');
      expect(relativeDate(now.subtract(const Duration(minutes: 5)), now), 'hace 5 min');
      expect(relativeDate(now.subtract(const Duration(hours: 3)), now), 'hace 3 h');
    });

    test('names yesterday, then the days of the week', () {
      expect(
        relativeDate(DateTime(2026, 10, 28, 23, 0), now),
        'ayer',
      );
      expect(relativeDate(DateTime(2026, 10, 25, 9, 0), now), 'hace 4 días');
    });

    test('writes an older day as day/month/year', () {
      expect(relativeDate(DateTime(2026, 9, 3, 8, 0), now), '03/09/2026');
    });

    test('a moment in the future reads as now', () {
      expect(relativeDate(now.add(const Duration(minutes: 3)), now), 'ahora');
    });
  });

  test('a note is titled by its subject, else by its text, else by a fallback', () {
    final stamp = DateTime(2026, 10, 29);
    AgendaNoteRow row(String? subject, String excerpt) => AgendaNoteRow(
      id: 'nt_1',
      subject: subject,
      excerpt: excerpt,
      updatedAt: stamp,
    );

    expect(noteTitle(row('Cliente', 'presupuesto')), 'Cliente');
    expect(noteTitle(row('  ', 'presupuesto 1.250 €')), 'presupuesto 1.250 €');
    expect(noteTitle(row(null, '')), 'Nota sin texto');
  });

  test('a multi-line field becomes its non-empty trimmed lines', () {
    expect(splitLines(' ana@mail.es \n\n  luis@mail.es\n'), [
      'ana@mail.es',
      'luis@mail.es',
    ]);
    expect(splitLines(' \n '), isEmpty);
  });

  test('an import summary has the four counts and no address', () {
    const report = AgendaImportReport(created: 2, merged: 1, skipped: 1, rejected: 3);

    expect(
      importSummary(report),
      '2 nuevos, 1 ampliados, 1 omitidos, 3 rechazados',
    );
  });

  test('a source word maps to its label, and an unknown word to none', () {
    expect(AgendaSource.fromWire('phone')?.label, 'del teléfono');
    expect(AgendaSource.fromWire('user_message')?.label, 'dictada');
    expect(AgendaSource.fromWire('alfred')?.label, 'Alfred');
    expect(AgendaSource.fromWire('app')?.label, 'creada aquí');
    expect(AgendaSource.fromWire('otro'), isNull);
    expect(AgendaSource.fromWire(null), isNull);
  });
}
