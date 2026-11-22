import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/state/calendar_time.dart';

void main() {
  final madrid = zoneNamed('Europe/Madrid')!;

  group('reloj de la empresa', () {
    test('un nombre que el teléfono no conoce no da zona', () {
      expect(zoneNamed('Mars/Olympus'), isNull);
    });

    test('al retrasar la hora se repiten las 02:30, y cada una es su instante', () {
      // Madrid pasa de CEST a CET el 25 de octubre de 2026 a las 03:00 locales.
      final first = wallClock(DateTime.utc(2026, 10, 25, 0, 30), madrid);
      final second = wallClock(DateTime.utc(2026, 10, 25, 1, 30), madrid);

      expect(clockInput(first), '2026-10-25T02:30');
      expect(clockInput(second), '2026-10-25T02:30');
    });

    test('al adelantar la hora la 02:30 no existe: el reloj salta a las 03:00', () {
      // El 29 de marzo de 2026 Madrid pasa de CET a CEST a las 02:00 locales.
      expect(
        clockInput(wallClock(DateTime.utc(2026, 3, 29, 0, 59), madrid)),
        '2026-03-29T01:59',
      );
      expect(
        clockInput(wallClock(DateTime.utc(2026, 3, 29, 1), madrid)),
        '2026-03-29T03:00',
      );
    });

    test('el día es el de la zona, no el de UTC', () {
      // 23:30 UTC del 28 es 00:30 del 29 en Madrid.
      expect(
        dayText(wallDay(DateTime.utc(2026, 10, 28, 23, 30), madrid)),
        '2026-10-29',
      );
    });

    test('la siguiente hora entera cruza la medianoche', () {
      expect(
        clockInput(nextFullHour(DateTime.utc(2026, 10, 31, 23, 40))),
        '2026-11-01T00:00',
      );
    });
  });

  group('días', () {
    test('sumar días cruza fin de mes sin tocar la hora', () {
      expect(
        addDays(DateTime.utc(2026, 10, 31), 1),
        DateTime.utc(2026, 11, 1),
      );
      expect(
        addDays(DateTime.utc(2026, 1, 1), -1),
        DateTime.utc(2025, 12, 31),
      );
    });

    test('el texto de un día y de un reloj', () {
      expect(dayText(DateTime.utc(2026, 1, 5)), '2026-01-05');
      expect(clockInput(DateTime.utc(2026, 11, 3, 9, 30)), '2026-11-03T09:30');
    });

    test('la ventana va de un día inclusive a otro exclusive', () {
      final window = windowOf(DateTime.utc(2026, 10, 29), CalendarRange.week);

      expect(window.from, '2026-10-29');
      expect(window.to, '2026-11-05');
    });

    test('desplazar la ventana mueve un rango entero', () {
      final anchor = DateTime.utc(2026, 10, 29);

      expect(
        shiftAnchor(anchor, CalendarRange.today, -1),
        DateTime.utc(2026, 10, 28),
      );
      expect(
        shiftAnchor(anchor, CalendarRange.month, 1),
        DateTime.utc(2026, 11, 28),
      );
    });
  });
}
