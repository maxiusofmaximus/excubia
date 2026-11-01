import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/agenda_api.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/ui/agenda/agenda_widgets.dart';
import 'package:herdr_notify/ui/agenda/calendar_screen.dart';

import '../support/scripted_transport.dart';
import '../support/widget_mount.dart';

const String _eventsPath = 'GET /v1/calendar/events';
const String _probePath = 'POST /v1/contacts/ct_grant_probe/delete';
const String _noPermission =
    'Falta un permiso de este dispositivo para esta acción. Pide a quien administra el equipo que lo conceda.';

/// The phone's clock: 29 October 2026, 09:00 in Madrid.
DateTime _now() => DateTime.utc(2026, 10, 29, 8);

/// Starts before the other: an all-day event of 29 October (local midnight to midnight).
const Map<String, Object?> _allDayRow = {
  'id': 'ev_2',
  'title': 'Festivo local',
  'start_utc': '2026-10-28T23:00:00.000Z',
  'end_utc': '2026-10-29T23:00:00.000Z',
  'all_day': true,
  'timezone': 'Europe/Madrid',
  'location': '',
  'notes': '',
  'reminder_minutes': null,
  'updated_at': '2026-10-29T09:00:00.000Z',
};

const Map<String, Object?> _timedRow = {
  'id': 'ev_1',
  'title': 'Llamada con Ana',
  'start_utc': '2026-10-29T08:30:00.000Z',
  'end_utc': null,
  'all_day': false,
  'timezone': 'Europe/Madrid',
  'location': 'Oficina',
  'notes': '',
  'reminder_minutes': 15,
  'updated_at': '2026-10-29T09:00:00.000Z',
};

/// One occurrence of a weekly series: the list shows the occurrence, and the stored
/// series starts a week earlier.
const Map<String, Object?> _seriesRow = {
  'id': 'ev_1',
  'title': 'Equipo',
  'start_utc': '2026-10-29T08:30:00.000Z',
  'end_utc': null,
  'all_day': false,
  'recurrence': 'FREQ=WEEKLY;INTERVAL=1;BYDAY=TH;COUNT=10',
  'timezone': 'Europe/Madrid',
  'location': '',
  'notes': '',
  'reminder_minutes': null,
  'updated_at': '2026-10-29T09:00:00.000Z',
  'occurrence_start': '2026-10-29T08:30:00.000Z',
  'recurring': true,
};

const String _seriesGetPath = 'GET /v1/calendar/events/ev_1';
const String _seriesUpdatePath = 'POST /v1/calendar/events/ev_1/update';
const String _secondPage =
    'GET /v1/calendar/events?from=2026-10-29&to=2026-10-30&limit=20&after=ev_1';

Future<void> _mount(WidgetTester tester, ScriptedTransport transport) async {
  await mountWidget(
    tester,
    CalendarScreen(api: AgendaApi(transport), clock: _now),
  );
  await tester.pumpAndSettle();
}

void main() {
  group('zona', () {
    testWidgets('sin zona lo avisa y Nuevo evento está apagado', (tester) async {
      final transport = ScriptedTransport({
        'GET /v1/company/timezone': okJson({'timezone': null}),
      });

      await _mount(tester, transport);

      expect(find.text('La empresa aún no tiene zona horaria'), findsOneWidget);
      final button = tester.widget<FloatingActionButton>(
        find.byType(FloatingActionButton),
      );
      expect(button.onPressed, isNull);
      expect(transport.calls, ['GET /v1/company/timezone']);
      await unmountWidget(tester);
    });

    testWidgets('una zona escrita a mano se guarda y la agenda sale en ella', (tester) async {
      final transport = ScriptedTransport({
        'GET /v1/company/timezone': okJson({'timezone': null}),
        'POST /v1/company/timezone': okJson({'timezone': 'Asia/Tokyo'}),
        _eventsPath: okJson({'events': [], 'next': null}),
      });

      await _mount(tester, transport);
      await tester.enterText(
        find.widgetWithText(TextField, 'O escribe otra (p. ej. Asia/Tokyo)'),
        'Asia/Tokyo',
      );
      await tester.tap(find.text('Guardar zona'));
      await tester.pumpAndSettle();

      expect(transport.bodies['POST /v1/company/timezone'], [
        {'timezone': 'Asia/Tokyo'},
      ]);
      expect(find.text('Horas de Asia/Tokyo'), findsOneWidget);
      await unmountWidget(tester);
    });
  });

  group('lista', () {
    testWidgets('con zona, el día tiene su encabezado y cada hora va en su sitio', (tester) async {
      final transport = ScriptedTransport({
        'GET /v1/company/timezone': okJson({'timezone': 'Europe/Madrid'}),
        _eventsPath: okJson({
          'events': [_allDayRow, _timedRow],
          'next': null,
        }),
      });

      await _mount(tester, transport);

      expect(find.text('Hoy · jue 29 oct'), findsOneWidget);
      expect(find.text('Todo el día'), findsOneWidget);
      expect(find.text('09:30'), findsOneWidget);
      expect(find.text('Llamada con Ana'), findsOneWidget);
      expect(find.text('Oficina'), findsOneWidget);
      final button = tester.widget<FloatingActionButton>(
        find.byType(FloatingActionButton),
      );
      expect(button.onPressed, isNotNull);
      await unmountWidget(tester);
    });

    testWidgets('sin eventos en el periodo lo dice', (tester) async {
      final transport = ScriptedTransport({
        'GET /v1/company/timezone': okJson({'timezone': 'Europe/Madrid'}),
        _eventsPath: okJson({'events': [], 'next': null}),
      });

      await _mount(tester, transport);

      expect(
        find.text('Nada en este periodo. Crea un evento con «Nuevo evento».'),
        findsOneWidget,
      );
      await unmountWidget(tester);
    });

    testWidgets('sin permiso de lectura lo dice, y Reintentar vuelve a leer', (tester) async {
      final transport = ScriptedTransport({
        'GET /v1/company/timezone': okJson({'timezone': 'Europe/Madrid'}),
        _eventsPath: okJson({'events': [], 'next': null}),
      });
      transport.failNext(_eventsPath, const GatewayError(403, ''));

      await _mount(tester, transport);

      expect(find.text(_noPermission), findsOneWidget);
      await tester.tap(find.text('Reintentar'));
      await tester.pumpAndSettle();

      expect(find.text(_noPermission), findsNothing);
      expect(transport.calls.where((call) => call == _eventsPath), hasLength(2));
      await unmountWidget(tester);
    });
  });

  group('editor', () {
    testWidgets('un evento sin título no se envía y lo dice', (tester) async {
      final transport = ScriptedTransport({
        'GET /v1/company/timezone': okJson({'timezone': 'Europe/Madrid'}),
        _eventsPath: okJson({'events': [], 'next': null}),
      });

      await _mount(tester, transport);
      await tester.tap(find.widgetWithText(FloatingActionButton, 'Nuevo evento'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Guardar'));
      await tester.pumpAndSettle();

      expect(find.text('El título es obligatorio.'), findsOneWidget);
      expect(transport.calls.where((call) => call.startsWith('POST')), isEmpty);
      await unmountWidget(tester);
    });

    testWidgets('sin APPROVE, Borrar evento está apagado y dice por qué', (tester) async {
      final transport = ScriptedTransport({
        'GET /v1/company/timezone': okJson({'timezone': 'Europe/Madrid'}),
        _eventsPath: okJson({'events': [_timedRow], 'next': null}),
      });
      transport.failNext(_probePath, const GatewayError(403, ''));

      await _mount(tester, transport);
      await tester.tap(find.text('Llamada con Ana'));
      await tester.pumpAndSettle();

      expect(find.text(approveMissingText), findsOneWidget);
      final button = tester.widget<OutlinedButton>(
        find.widgetWithText(OutlinedButton, 'Borrar evento'),
      );
      expect(button.onPressed, isNull);
      await unmountWidget(tester);
    });
  });

  group('repetición', () {
    const timezone = 'GET /v1/company/timezone';
    const newEvent = 'POST /v1/calendar/events';
    const probe = _probePath;

    testWidgets('una serie sale con su marca, y Cargar más pide la página siguiente', (tester) async {
      final transport = ScriptedTransport({
        timezone: okJson({'timezone': 'Europe/Madrid'}),
        _eventsPath: okJson({
          'events': [_seriesRow],
          'truncated': true,
          'next': 'ev_1',
        }),
        _secondPage: okJson({
          'events': [_timedRow],
          'truncated': false,
          'next': null,
        }),
      });

      await _mount(tester, transport);

      expect(find.text('Equipo'), findsOneWidget);
      expect(find.byIcon(Icons.repeat), findsOneWidget);
      await tester.tap(find.text('Cargar más'));
      await tester.pumpAndSettle();

      expect(find.text('Llamada con Ana'), findsOneWidget);
      expect(find.text('Cargar más'), findsNothing);
      expect(transport.calls.where((call) => call == _eventsPath), hasLength(2));
      await unmountWidget(tester);
    });

    testWidgets('truncada y sin filas, pide acotar las fechas', (tester) async {
      final transport = ScriptedTransport({
        timezone: okJson({'timezone': 'Europe/Madrid'}),
        _eventsPath: okJson({'events': [], 'truncated': true, 'next': 'ev_0'}),
      });

      await _mount(tester, transport);

      expect(
        find.text('Hay demasiados eventos en este periodo, acota las fechas.'),
        findsOneWidget,
      );
      expect(find.text('Cargar más'), findsNothing);
      await unmountWidget(tester);
    });

    testWidgets('el formulario guarda la regla que se eligió', (tester) async {
      final transport = ScriptedTransport({
        timezone: okJson({'timezone': 'Europe/Madrid'}),
        _eventsPath: okJson({'events': [], 'truncated': false, 'next': null}),
        newEvent: okJson({'event': _timedRow}),
      });

      await _mount(tester, transport);
      await tester.tap(find.widgetWithText(FloatingActionButton, 'Nuevo evento'));
      await tester.pumpAndSettle();
      await tester.enterText(find.widgetWithText(TextField, 'Título'), 'Equipo');
      await tester.tap(find.text('No se repite'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Cada semana'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Guardar'));
      await tester.pumpAndSettle();

      expect(
        transport.bodies[newEvent]!.single['recurrence'],
        'FREQ=WEEKLY;INTERVAL=1;BYDAY=TH',
      );
      await unmountWidget(tester);
    });

    testWidgets('marcar un día de la semana lo añade a la regla', (tester) async {
      final transport = ScriptedTransport({
        timezone: okJson({'timezone': 'Europe/Madrid'}),
        _eventsPath: okJson({'events': [], 'truncated': false, 'next': null}),
        newEvent: okJson({'event': _timedRow}),
      });

      await _mount(tester, transport);
      await tester.tap(find.widgetWithText(FloatingActionButton, 'Nuevo evento'));
      await tester.pumpAndSettle();
      await tester.enterText(find.widgetWithText(TextField, 'Título'), 'Equipo');
      await tester.tap(find.text('No se repite'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Cada semana'));
      await tester.pumpAndSettle();
      await tester.tap(find.widgetWithText(FilterChip, 'X'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Guardar'));
      await tester.pumpAndSettle();

      expect(
        transport.bodies[newEvent]!.single['recurrence'],
        'FREQ=WEEKLY;INTERVAL=1;BYDAY=WE,TH',
      );
      await unmountWidget(tester);
    });

    testWidgets('editar una ocurrencia edita la serie: guarda su inicio, no el de la ocurrencia', (tester) async {
      final transport = ScriptedTransport({
        timezone: okJson({'timezone': 'Europe/Madrid'}),
        _eventsPath: okJson({'events': [_seriesRow], 'truncated': false, 'next': null}),
        _seriesGetPath: okJson({
          'event': {
            ..._seriesRow,
            'start_utc': '2026-10-22T08:30:00.000Z',
          },
        }),
        _seriesUpdatePath: okJson({'event': _timedRow}),
        probe: okJson({}),
      });

      await _mount(tester, transport);
      await tester.tap(find.text('Equipo'));
      await tester.pumpAndSettle();

      expect(find.text('Al editar o borrar se cambia toda la serie.'), findsOneWidget);
      await tester.tap(find.text('Guardar'));
      await tester.pumpAndSettle();

      expect(transport.bodies[_seriesUpdatePath]!.single['start'], '2026-10-22T10:30');
      await unmountWidget(tester);
    });

    testWidgets('borrar una serie avisa de que se borran todas las repeticiones', (tester) async {
      final transport = ScriptedTransport({
        timezone: okJson({'timezone': 'Europe/Madrid'}),
        _eventsPath: okJson({'events': [_seriesRow], 'truncated': false, 'next': null}),
        _seriesGetPath: okJson({'event': _seriesRow}),
        probe: okJson({}),
      });

      await _mount(tester, transport);
      await tester.tap(find.text('Equipo'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Borrar evento'));
      await tester.pumpAndSettle();

      expect(
        find.text('Se borran todas las repeticiones. No se puede deshacer.'),
        findsOneWidget,
      );
      await unmountWidget(tester);
    });
  });
}
