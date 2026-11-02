import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/agenda_api.dart';
import 'package:herdr_notify/core/api/host_client.dart';

import '../support/ics_fakes.dart';
import '../support/scripted_transport.dart';

const String _ics = 'BEGIN:VCALENDAR\nVERSION:2.0\nEND:VCALENDAR\n';
const String _exportPath = 'GET /v1/calendar/export.ics';
const String _importPath = 'POST /v1/calendar/import';
const String _emptyImport =
    '{"ok":true,"imported":0,"skipped_existing":0,"skipped_unsupported":0,"details":[]}';

HostError _error(int status, String code) => HostError(
  status,
  jsonEncode({'ok': false, 'detail': {'code': code}}),
);

void main() {
  late IcsTransport transport;
  late AgendaApi api;

  setUp(() {
    transport = IcsTransport(ScriptedTransport({_exportPath: _ics}));
    api = AgendaApi(transport);
  });

  group('exportar', () {
    test('sin ventana pide todo y devuelve el archivo tal cual', () async {
      final bytes = await api.exportIcs();

      expect(utf8.decode(bytes), _ics);
      expect(transport.targets, ['GET /v1/calendar/export.ics']);
    });

    test('con ventana pide from y to, y to no se incluye', () async {
      await api.exportIcs((from: '2026-10-29', to: '2026-11-05'));

      expect(transport.targets, [
        'GET /v1/calendar/export.ics?from=2026-10-29&to=2026-11-05',
      ]);
    });

    test('un 422 too_many_events llega con su código', () async {
      transport.json.failNext(_exportPath, _error(422, 'too_many_events'));

      await expectLater(
        api.exportIcs(),
        throwsA(
          isA<HostError>().having((e) => e.status, 'status', 422),
        ),
      );
    });
  });

  group('importar', () {
    test('manda el archivo tal cual, como text/calendar, y lee el resultado', () async {
      transport.json.routes[_importPath] = jsonEncode({
        'ok': true,
        'imported': 2,
        'skipped_existing': 1,
        'skipped_unsupported': 1,
        'details': [
          {'index': 2, 'reason': 'exists'},
          {'index': 3, 'reason': 'exception_instance'},
        ],
      });

      final report = await api.importIcs(Uint8List.fromList(utf8.encode(_ics)));

      final post = transport.posts.single;
      expect(post.target, '/v1/calendar/import');
      expect(post.contentType, 'text/calendar');
      expect(utf8.decode(post.bytes), _ics);
      expect(report.imported, 2);
      expect(report.skippedExisting, 1);
      expect(report.skippedUnsupported, 1);
      expect(report.details.map((skip) => skip.index), [2, 3]);
      expect(report.details.map((skip) => skip.reason), [
        IcsReason.exists,
        IcsReason.exceptionInstance,
      ]);
    });

    test('un archivo de más de 512 KiB se rechaza sin enviarlo', () async {
      await expectLater(
        api.importIcs(Uint8List(icsMaxBytes + 1)),
        throwsA(isA<HostError>().having((e) => e.status, 'status', 413)),
      );

      expect(transport.targets, isEmpty);
    });

    test('un archivo de exactamente 512 KiB sí se envía', () async {
      transport.json.routes[_importPath] = _emptyImport;

      await api.importIcs(Uint8List(icsMaxBytes));

      expect(transport.posts.single.bytes.length, icsMaxBytes);
    });

    test('un 400 invalid_ics llega con su código', () async {
      transport.json.failNext(_importPath, _error(400, 'invalid_ics'));

      await expectLater(
        api.importIcs(Uint8List.fromList(utf8.encode('hola'))),
        throwsA(isA<HostError>().having((e) => e.status, 'status', 400)),
      );
    });
  });

  group('motivos', () {
    test('cada código se dice en palabras; uno desconocido, sin motivo', () {
      expect(IcsReason.fromWire('exists').label, 'Ya existía');
      expect(
        IcsReason.fromWire('unsupported_recurrence').label,
        'Repetición no soportada',
      );
      expect(
        IcsReason.fromWire('exception_instance').label,
        'Ocurrencia suelta de una serie',
      );
      expect(IcsReason.fromWire('no_start').label, 'Sin fecha de inicio');
      expect(IcsReason.fromWire('invalid_time').label, 'Hora no válida');
      expect(IcsReason.fromWire('calendar_limit').label, 'Límite de eventos alcanzado');
      expect(IcsReason.fromWire('algo_nuevo'), IcsReason.other);
    });
  });

  group('textos de error', () {
    test('el límite de eventos dice cómo arreglarlo en cada sentido', () {
      final limit = _error(422, 'too_many_events');

      expect(
        icsFailureText(limit, importing: true),
        'Tiene más de 500 eventos: divide el archivo.',
      );
      expect(
        icsFailureText(limit, importing: false),
        'Hay demasiados eventos: elige un rango más corto.',
      );
    });

    test('un archivo que no es un calendario, y uno que pesa demasiado', () {
      expect(
        icsFailureText(_error(400, 'invalid_ics'), importing: true),
        'El archivo no es un calendario .ics válido.',
      );
      expect(
        icsFailureText(const HostError(413, ''), importing: true),
        'El archivo pesa más de 512 KiB.',
      );
    });

    test('sin zona lo dice y se puede reconocer para fijarla', () {
      final unset = _error(409, 'timezone_unset');

      expect(
        icsFailureText(unset, importing: false),
        'Antes hay que fijar la zona horaria de la empresa.',
      );
      expect(icsZoneUnset(unset), isTrue);
      expect(icsZoneUnset(_error(422, 'too_many_events')), isFalse);
    });

    test('el resto usa el texto de la agenda', () {
      final denied = _error(403, 'forbidden');

      expect(icsFailureText(denied, importing: true), agendaFailureText(denied));
    });
  });
}
