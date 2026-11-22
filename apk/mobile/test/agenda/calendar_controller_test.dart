import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/agenda_api.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/state/calendar_controller.dart';
import 'package:excubia/core/state/calendar_time.dart';

import '../support/recording_transport.dart';

DateTime _now() => DateTime.utc(2026, 10, 29, 8);

/// One answer serves both reads: the zone and an empty page of events.
const String _zoneAndEmptyPage =
    '{"ok":true,"timezone":"Europe/Madrid","events":[],"next":null}';

void main() {
  late RecordingTransport transport;
  late CalendarController controller;

  setUp(() {
    transport = RecordingTransport();
    controller = CalendarController(AgendaApi(transport), clock: _now);
    addTearDown(controller.dispose);
  });

  test('sin zona queda sin zona y no pide eventos', () async {
    transport.answer = '{"ok":true,"timezone":null}';

    await controller.loadZone();

    expect(controller.zone, isA<ZoneUnset>());
    expect(transport.targets, ['GET /v1/company/timezone']);
  });

  test('una zona que el teléfono no conoce se dice por su nombre', () async {
    transport.answer = '{"ok":true,"timezone":"Mars/Olympus"}';

    await controller.loadZone();

    expect(
      controller.zone,
      isA<ZoneUnknown>().having((zone) => zone.name, 'name', 'Mars/Olympus'),
    );
  });

  test('un fallo al fijar la zona la deja sin fijar y dice por qué', () async {
    transport.failure = const HostError(
      409,
      '{"detail":{"code":"timezone_already_set"}}',
    );

    await controller.saveZone('Europe/Madrid');

    expect(
      controller.zone,
      isA<ZoneUnset>()
          .having((zone) => zone.saving, 'saving', false)
          .having(
            (zone) => zone.failure,
            'failure',
            'La zona horaria de la empresa ya está fijada y no cambia.',
          ),
    );
  });

  test('con zona, la ventana empieza hoy y se mueve por días', () async {
    transport.answer = _zoneAndEmptyPage;

    await controller.loadZone();

    expect(
      controller.zone,
      isA<ZoneReady>()
          .having((zone) => zone.range, 'range', CalendarRange.today)
          .having((zone) => zone.anchor, 'anchor', DateTime.utc(2026, 10, 29)),
    );
    expect(transport.targets.last,
        'GET /v1/calendar/events?from=2026-10-29&to=2026-10-30&limit=20');

    controller.setRange(CalendarRange.week);
    expect(transport.targets.last,
        'GET /v1/calendar/events?from=2026-10-29&to=2026-11-05&limit=20');

    controller.shift(1);
    expect(transport.targets.last,
        'GET /v1/calendar/events?from=2026-11-05&to=2026-11-12&limit=20');

    controller.shift(-1);
    expect(transport.targets.last,
        'GET /v1/calendar/events?from=2026-10-29&to=2026-11-05&limit=20');
  });
}
