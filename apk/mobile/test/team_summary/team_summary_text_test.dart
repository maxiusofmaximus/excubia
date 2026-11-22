import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/team_summary_models.dart';
import 'package:excubia/ui/team/team_summary_text.dart';

void main() {
  test('each state has the words the desktop uses', () {
    expect(TeamStatus.values.map(teamStatusLabel), [
      'Espera tu visto bueno',
      'Necesita ayuda',
      'Trabajando',
      'Terminó',
      'Sin actividad',
    ]);
  });

  group('teamWaitingText', () {
    test('nobody waiting says so', () {
      expect(teamWaitingText(0), 'Nadie espera tu visto bueno.');
    });

    test('one waiting, in the singular', () {
      expect(teamWaitingText(1), '1 espera tu visto bueno');
    });

    test('several waiting, in the plural', () {
      expect(teamWaitingText(2), '2 esperan tu visto bueno');
    });
  });

  group('teamAgeLabel', () {
    final now = DateTime.utc(2026, 11, 5, 12);

    test('no time from the host gives no age', () {
      expect(teamAgeLabel(null, now), isNull);
    });

    test('under a minute says a moment', () {
      expect(
        teamAgeLabel(now.subtract(const Duration(seconds: 40)), now),
        'desde hace un momento',
      );
    });

    test('minutes', () {
      expect(
        teamAgeLabel(now.subtract(const Duration(minutes: 12)), now),
        'desde hace 12 min',
      );
    });

    test('hours', () {
      expect(
        teamAgeLabel(now.subtract(const Duration(hours: 3)), now),
        'desde hace 3 h',
      );
    });

    test('one day, then days', () {
      expect(
        teamAgeLabel(now.subtract(const Duration(hours: 25)), now),
        'desde hace 1 día',
      );
      expect(
        teamAgeLabel(now.subtract(const Duration(days: 4)), now),
        'desde hace 4 días',
      );
    });

    test(
      'a time in the future, from a clock that runs behind, says a moment',
      () {
        expect(
          teamAgeLabel(now.add(const Duration(minutes: 2)), now),
          'desde hace un momento',
        );
      },
    );

    test('the same instant in local time gives the same age', () {
      final since = now.subtract(const Duration(minutes: 12));
      expect(teamAgeLabel(since.toLocal(), now), 'desde hace 12 min');
    });
  });

  group('teamShownHeadline', () {
    test('an empty headline is not shown', () {
      expect(teamShownHeadline('   ', 'Sin actividad'), '');
    });

    test('a headline that only repeats the state is not shown', () {
      expect(teamShownHeadline('Sin actividad', 'Sin actividad'), '');
      expect(teamShownHeadline('  sin ACTIVIDAD. ', 'Sin actividad'), '');
    });

    test('a headline with other words is shown as it came', () {
      expect(
        teamShownHeadline('Revisando el presupuesto', 'Trabajando'),
        'Revisando el presupuesto',
      );
      expect(teamShownHeadline('Sin actividad', 'Trabajando'), 'Sin actividad');
    });
  });
}
