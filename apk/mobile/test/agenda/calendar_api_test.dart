import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/agenda_api.dart';
import 'package:herdr_notify/core/api/host_client.dart';

import '../support/recording_transport.dart';

const String _eventRow =
    '{"id":"ev_1","title":"Llamada con Ana","start_utc":"2026-10-29T08:30:00.000Z",'
    '"end_utc":null,"all_day":false,"timezone":"Europe/Madrid","location":"",'
    '"notes":"","reminder_minutes":15,"updated_at":"2026-10-29T09:00:00.000Z"}';

const String _seriesRow =
    '{"id":"ev_1","title":"Equipo","start_utc":"2026-10-29T08:30:00.000Z",'
    '"end_utc":null,"all_day":false,"recurrence":"FREQ=WEEKLY;INTERVAL=1;BYDAY=TH;COUNT=10",'
    '"timezone":"Europe/Madrid","location":"","notes":"","reminder_minutes":null,'
    '"updated_at":"2026-10-29T09:00:00.000Z","occurrence_start":"2026-10-29T08:30:00.000Z",'
    '"recurring":true}';

String _detail(int status, Map<String, Object?> detail) =>
    agendaFailureText(HostError(status, jsonEncode({'detail': detail})));

void main() {
  late RecordingTransport transport;
  late AgendaApi api;

  setUp(() {
    transport = RecordingTransport();
    api = AgendaApi(transport);
  });

  group('eventos', () {
    test('lee la ventana de días y pasa el cursor de la página anterior', () async {
      transport.answer = '{"ok":true,"events":[$_eventRow],"next":"ev_1"}';

      final page = await api.calendarEvents(
        from: '2026-10-29',
        to: '2026-11-05',
        after: 'ev_0',
      );

      expect(
        transport.targets.single,
        'GET /v1/calendar/events?from=2026-10-29&to=2026-11-05&limit=20&after=ev_0',
      );
      final event = page.items.single;
      expect(event.title, 'Llamada con Ana');
      expect(event.start, DateTime.utc(2026, 10, 29, 8, 30));
      expect(event.end, isNull);
      expect(event.reminderMinutes, 15);
      expect(page.next, 'ev_1');
    });

    test('crear envía solo lo que tiene valor', () async {
      transport.answer = '{"ok":true,"event":$_eventRow}';

      await api.createEvent(
        const CalendarEventInput(
          title: 'Llamada',
          allDay: false,
          start: '2026-11-03T09:30',
        ),
      );

      expect(transport.targets.single, 'POST /v1/calendar/events');
      expect(transport.bodies.single, {
        'title': 'Llamada',
        'all_day': false,
        'start': '2026-11-03T09:30',
      });
    });

    test('editar manda vacíos y -1 para quitar fin, lugar, notas y aviso', () async {
      transport.answer = '{"ok":true,"event":$_eventRow}';

      await api.updateEvent(
        'ev_1',
        const CalendarEventInput(
          title: 'Llamada',
          allDay: true,
          start: '2026-11-03',
          end: '2026-11-03',
        ),
      );

      expect(transport.targets.single, 'POST /v1/calendar/events/ev_1/update');
      expect(transport.bodies.single, {
        'title': 'Llamada',
        'all_day': true,
        'start': '2026-11-03',
        'end': '2026-11-03',
        'location': '',
        'notes': '',
        'reminder_minutes': -1,
      });
    });

    test('editar sin fin manda fin vacío', () async {
      transport.answer = '{"ok":true,"event":$_eventRow}';

      await api.updateEvent(
        'ev_1',
        const CalendarEventInput(title: 'x', allDay: false, start: '2026-11-03T09:30'),
      );

      expect(transport.bodies.single['end'], '');
    });

    test('borrar va al id del evento', () async {
      transport.answer = '{"ok":true,"id":"ev_1"}';

      await api.deleteEvent('ev_1');

      expect(transport.targets.single, 'POST /v1/calendar/events/ev_1/delete');
    });

    test('una serie trae su regla, su ocurrencia y el cursor de la página', () async {
      transport.answer =
          '{"ok":true,"events":[$_seriesRow],"truncated":true,"next":"ev_1@2026-11-05T08:30:00.000Z"}';

      final page = await api.calendarEvents(from: '2026-10-29', to: '2026-11-05');

      final event = page.items.single;
      expect(event.recurrence, 'FREQ=WEEKLY;INTERVAL=1;BYDAY=TH;COUNT=10');
      expect(event.recurring, isTrue);
      expect(event.occurrenceStart, DateTime.utc(2026, 10, 29, 8, 30));
      expect(page.truncated, isTrue);
      expect(page.next, 'ev_1@2026-11-05T08:30:00.000Z');
    });

    test('una fila sin campos de repetición sale sin regla y sin truncar', () async {
      transport.answer = '{"ok":true,"events":[$_eventRow],"next":null}';

      final page = await api.calendarEvents(from: '2026-10-29', to: '2026-11-05');

      final event = page.items.single;
      expect(event.recurrence, isNull);
      expect(event.recurring, isFalse);
      expect(event.occurrenceStart, isNull);
      expect(page.truncated, isFalse);
    });

    test('leer un evento va a su id y devuelve la serie entera', () async {
      transport.answer = '{"ok":true,"event":$_seriesRow}';

      final event = await api.calendarEvent('ev_1');

      expect(transport.targets.single, 'GET /v1/calendar/events/ev_1');
      expect(event.recurrence, 'FREQ=WEEKLY;INTERVAL=1;BYDAY=TH;COUNT=10');
    });

    test('crear manda la regla; una regla vacía no se manda', () async {
      transport.answer = '{"ok":true,"event":$_seriesRow}';

      await api.createEvent(
        const CalendarEventInput(
          title: 'Equipo',
          allDay: false,
          start: '2026-10-29T10:00',
          recurrence: 'FREQ=WEEKLY;INTERVAL=1;BYDAY=TH',
        ),
      );
      await api.createEvent(
        const CalendarEventInput(
          title: 'Equipo',
          allDay: false,
          start: '2026-10-29T10:00',
          recurrence: '',
        ),
      );

      expect(transport.bodies[0]['recurrence'], 'FREQ=WEEKLY;INTERVAL=1;BYDAY=TH');
      expect(transport.bodies[1].containsKey('recurrence'), isFalse);
    });

    test('editar con regla vacía la quita; sin regla la deja como está', () async {
      transport.answer = '{"ok":true,"event":$_seriesRow}';

      await api.updateEvent(
        'ev_1',
        const CalendarEventInput(title: 'Equipo', allDay: false, start: '2026-10-29T10:00', recurrence: ''),
      );
      await api.updateEvent(
        'ev_1',
        const CalendarEventInput(title: 'Equipo', allDay: false, start: '2026-10-29T10:00'),
      );

      expect(transport.bodies[0]['recurrence'], '');
      expect(transport.bodies[1].containsKey('recurrence'), isFalse);
    });
  });

  group('zona de la empresa', () {
    test('sin zona fijada la lectura da null', () async {
      transport.answer = '{"ok":true,"timezone":null}';

      expect(await api.companyTimezone(), isNull);
      expect(transport.targets.single, 'GET /v1/company/timezone');
    });

    test('fijar la zona manda el nombre IANA y devuelve el que guardó', () async {
      transport.answer = '{"ok":true,"timezone":"Europe/Madrid"}';

      final zone = await api.setCompanyTimezone('Europe/Madrid');

      expect(transport.targets.single, 'POST /v1/company/timezone');
      expect(transport.bodies.single, {'timezone': 'Europe/Madrid'});
      expect(zone, 'Europe/Madrid');
    });
  });

  group('errores del calendario', () {
    test('zona sin fijar y zona ya fijada', () {
      expect(
        _detail(409, {'code': 'timezone_unset'}),
        'Antes hay que fijar la zona horaria de la empresa.',
      );
      expect(
        _detail(409, {'code': 'timezone_already_set'}),
        'La zona horaria de la empresa ya está fijada y no cambia.',
      );
    });

    test('una zona que no existe, con su código o con el campo', () {
      const zone = 'Esa zona no existe. Usa un nombre como Europe/Madrid.';
      expect(_detail(422, {'code': 'invalid_zone', 'field': 'timezone'}), zone);
      expect(
        _detail(422, {'code': 'invalid_field', 'field': 'timezone'}),
        zone,
      );
    });

    test('una hora que el cambio de horario salta', () {
      expect(
        _detail(422, {'code': 'invalid_local_time', 'field': 'start'}),
        'Esa hora no existe en la zona de la empresa: el cambio de horario la salta.',
      );
    });

    test('los límites del calendario', () {
      expect(
        _detail(409, {'code': 'calendar_limit'}),
        'Ya hay 5000 eventos. Borra alguno para crear otro.',
      );
      expect(
        _detail(422, {'code': 'invalid_field', 'field': 'title'}),
        'El título es obligatorio y tiene 120 caracteres como máximo.',
      );
      expect(
        _detail(422, {'code': 'invalid_field', 'field': 'location'}),
        'El lugar tiene 200 caracteres como máximo.',
      );
    });

    test('un evento que ya no existe se dice en español', () {
      expect(
        agendaFailureText(const HostError(404, '{"detail":{"code":"not_found"}}')),
        'Ya no existe. Vuelve a la lista.',
      );
    });
  });
}
