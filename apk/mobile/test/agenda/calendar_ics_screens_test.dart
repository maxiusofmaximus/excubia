import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/agenda_api.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/ui/agenda/calendar_screen.dart';

import '../support/ics_fakes.dart';
import '../support/scripted_transport.dart';
import '../support/widget_mount.dart';

const String _ics = 'BEGIN:VCALENDAR\nVERSION:2.0\nEND:VCALENDAR\n';
const String _eventsPath = 'GET /v1/calendar/events';
const String _exportPath = 'GET /v1/calendar/export.ics';
const String _importPath = 'POST /v1/calendar/import';
const String _zonePath = 'GET /v1/company/timezone';

const String _importAnswer =
    '{"ok":true,"imported":2,"skipped_existing":1,"skipped_unsupported":1,'
    '"details":[{"index":2,"reason":"exists"},'
    '{"index":3,"reason":"unsupported_recurrence"}]}';

/// The phone's clock: 29 October 2026, 09:00 in Madrid. «Hoy» is 29 October.
DateTime _now() => DateTime.utc(2026, 10, 29, 8);

ScriptedTransport _host() => ScriptedTransport({
  _zonePath: okJson({'timezone': 'Europe/Madrid'}),
  _eventsPath: okJson({'events': [], 'next': null}),
  _exportPath: _ics,
  _importPath: _importAnswer,
});

HostError _refused(int status, String code) => HostError(
  status,
  jsonEncode({'ok': false, 'detail': {'code': code}}),
);

Future<void> _mount(
  WidgetTester tester,
  IcsTransport transport,
  FakeIcsFiles files,
) async {
  await mountWidget(
    tester,
    CalendarScreen(api: AgendaApi(transport), clock: _now, files: files),
  );
  await tester.pumpAndSettle();
}

Future<void> _choose(WidgetTester tester, String item) async {
  await tester.tap(find.byTooltip('Archivo .ics'));
  await tester.pumpAndSettle();
  await tester.tap(find.text(item));
  await tester.pumpAndSettle();
}

int _count(IcsTransport transport, String call) =>
    transport.json.calls.where((made) => made == call).length;

void main() {
  testWidgets('el menú del archivo ofrece importar y exportar', (tester) async {
    final transport = IcsTransport(_host());
    await _mount(tester, transport, FakeIcsFiles());

    await tester.tap(find.byTooltip('Archivo .ics'));
    await tester.pumpAndSettle();

    expect(find.text('Importar .ics'), findsOneWidget);
    expect(find.text('Exportar rango visible'), findsOneWidget);
    expect(find.text('Exportar todo'), findsOneWidget);
    await unmountWidget(tester);
  });

  group('importar', () {
    testWidgets('el archivo elegido se envía y el resultado cuenta y explica', (tester) async {
      final transport = IcsTransport(_host());
      final files = FakeIcsFiles(picked: Uint8List.fromList(utf8.encode(_ics)));
      await _mount(tester, transport, files);

      await _choose(tester, 'Importar .ics');

      expect(transport.posts.single.contentType, 'text/calendar');
      expect(utf8.decode(transport.posts.single.bytes), _ics);
      expect(find.text('2 importados · 1 ya existía · 1 omitido'), findsOneWidget);
      expect(find.text('Evento 3: Ya existía'), findsOneWidget);
      expect(find.text('Evento 4: Repetición no soportada'), findsOneWidget);
      expect(_count(transport, _eventsPath), 2, reason: 'la lista se vuelve a leer');
      await unmountWidget(tester);
    });

    testWidgets('cancelar el selector no hace nada', (tester) async {
      final transport = IcsTransport(_host());
      await _mount(tester, transport, FakeIcsFiles());

      await _choose(tester, 'Importar .ics');

      expect(transport.posts, isEmpty);
      expect(find.textContaining('importados'), findsNothing);
      await unmountWidget(tester);
    });

    testWidgets('un archivo de más de 512 KiB se avisa sin enviarlo', (tester) async {
      final transport = IcsTransport(_host());
      final files = FakeIcsFiles(picked: Uint8List(512 * 1024 + 1));
      await _mount(tester, transport, files);

      await _choose(tester, 'Importar .ics');

      expect(find.text('El archivo pesa más de 512 KiB.'), findsOneWidget);
      expect(transport.posts, isEmpty);
      await unmountWidget(tester);
    });

    testWidgets('un .ics que no es un calendario se dice', (tester) async {
      final transport = IcsTransport(_host());
      transport.json.failNext(_importPath, _refused(400, 'invalid_ics'));
      final files = FakeIcsFiles(picked: Uint8List.fromList(utf8.encode('hola')));
      await _mount(tester, transport, files);

      await _choose(tester, 'Importar .ics');

      expect(find.text('El archivo no es un calendario .ics válido.'), findsOneWidget);
      await unmountWidget(tester);
    });

    testWidgets('más de 500 eventos pide dividir el archivo', (tester) async {
      final transport = IcsTransport(_host());
      transport.json.failNext(_importPath, _refused(422, 'too_many_events'));
      final files = FakeIcsFiles(picked: Uint8List.fromList(utf8.encode(_ics)));
      await _mount(tester, transport, files);

      await _choose(tester, 'Importar .ics');

      expect(find.text('Tiene más de 500 eventos: divide el archivo.'), findsOneWidget);
      await unmountWidget(tester);
    });

    testWidgets('sin zona relee la zona, que lleva a fijarla', (tester) async {
      final transport = IcsTransport(_host());
      transport.json.failNext(_importPath, _refused(409, 'timezone_unset'));
      final files = FakeIcsFiles(picked: Uint8List.fromList(utf8.encode(_ics)));
      await _mount(tester, transport, files);

      await _choose(tester, 'Importar .ics');

      expect(
        find.text('Antes hay que fijar la zona horaria de la empresa.'),
        findsOneWidget,
      );
      expect(_count(transport, _zonePath), 2);
      await unmountWidget(tester);
    });
  });

  group('exportar', () {
    testWidgets('el rango visible manda sus días y se guarda como excubia.ics', (tester) async {
      final transport = IcsTransport(_host());
      final files = FakeIcsFiles();
      await _mount(tester, transport, files);

      await _choose(tester, 'Exportar rango visible');

      expect(
        transport.targets,
        contains('GET /v1/calendar/export.ics?from=2026-10-29&to=2026-10-30'),
      );
      expect(files.saved.single.name, 'excubia.ics');
      expect(utf8.decode(files.saved.single.bytes), _ics);
      expect(find.text('Archivo .ics guardado.'), findsOneWidget);
      await unmountWidget(tester);
    });

    testWidgets('todo no manda ventana', (tester) async {
      final transport = IcsTransport(_host());
      await _mount(tester, transport, FakeIcsFiles());

      await _choose(tester, 'Exportar todo');

      expect(transport.targets, contains('GET /v1/calendar/export.ics'));
      await unmountWidget(tester);
    });

    testWidgets('demasiados eventos piden un rango más corto', (tester) async {
      final transport = IcsTransport(_host());
      transport.json.failNext(_exportPath, _refused(422, 'too_many_events'));
      await _mount(tester, transport, FakeIcsFiles());

      await _choose(tester, 'Exportar todo');

      expect(
        find.text('Hay demasiados eventos: elige un rango más corto.'),
        findsOneWidget,
      );
      await unmountWidget(tester);
    });

    testWidgets('cancelar el guardado no es un error', (tester) async {
      final transport = IcsTransport(_host());
      final files = FakeIcsFiles(saveAccepted: false);
      await _mount(tester, transport, files);

      await _choose(tester, 'Exportar todo');

      expect(files.saved, hasLength(1));
      expect(find.text('Archivo .ics guardado.'), findsNothing);
      await unmountWidget(tester);
    });
  });
}
