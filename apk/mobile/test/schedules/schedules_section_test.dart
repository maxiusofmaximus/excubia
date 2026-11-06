import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/api/schedules_api.dart';
import 'package:herdr_notify/core/state/schedules_controller.dart';
import 'package:herdr_notify/ui/schedules/schedules_section.dart';
import 'package:herdr_notify/ui/theme.dart';

import '../support/scripted_transport.dart';

Map<String, Object?> _schedule({
  String status = 'active',
  String? rule = 'FREQ=WEEKLY;INTERVAL=1;BYDAY=MO',
  String? nextRunAt = '2026-11-09T08:00:00.000Z',
}) => {
  'id': 'sch_1',
  'title': 'Resumen del lunes',
  'conversation_id': 'conv_1',
  'role_id': 'audio-editor',
  'body': 'texto del encargo que no se muestra',
  'recipe_id': null,
  'rule': rule,
  'start_date': '2026-11-02',
  'time_local': '09:00',
  'timezone': 'Europe/Madrid',
  'steps': 1,
  'status': status,
  'plan_id': null,
  'next_run_at': nextRunAt,
  'last_run_at': null,
  'created_at': '2026-11-01T10:00:00.000Z',
  'updated_at': '2026-11-01T10:00:00.000Z',
};

String _list(List<Map<String, Object?>> schedules) =>
    jsonEncode({'ok': true, 'schedules': schedules});

/// Mounts the section after one read, at [width] dp and [textScale].
Future<SchedulesController> pumpSection(
  WidgetTester tester,
  ScriptedTransport host, {
  double width = 390,
  double textScale = 1,
}) async {
  final schedules = SchedulesController(
    api: SchedulesApi(host),
    interval: const Duration(hours: 1),
  );
  addTearDown(schedules.dispose);
  await schedules.refresh();
  tester.view.physicalSize = Size(width * 3, 2400 * 3);
  tester.view.devicePixelRatio = 3;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(context)
            .copyWith(textScaler: TextScaler.linear(textScale)),
        child: child!,
      ),
      home: Scaffold(
        body: ListView(
          children: [
            SchedulesSection(
              schedules: schedules,
              oficioOf: (roleId) => 'Editor de audio',
            ),
          ],
        ),
      ),
    ),
  );
  await tester.pump();
  return schedules;
}

void main() {
  testWidgets('with nothing scheduled it says to program from the desktop', (
    tester,
  ) async {
    final host = ScriptedTransport({'GET /v1/schedules': _list([])});

    await pumpSection(tester, host);

    expect(find.text('Aún no hay tareas programadas.'), findsOneWidget);
    expect(find.text('Programa tareas desde el escritorio.'), findsOneWidget);
    expect(find.text('Crear'), findsNothing);
    expect(
      find.textContaining('Solo se ejecuta con tu PC encendido.'),
      findsOneWidget,
    );
  });

  testWidgets('a card names the task, its oficio, its rule and its next run', (
    tester,
  ) async {
    final host = ScriptedTransport({
      'GET /v1/schedules': _list([_schedule()]),
    });

    await pumpSection(tester, host);

    expect(find.text('Resumen del lunes'), findsOneWidget);
    expect(
      find.text('Editor de audio · Cada lunes a las 9:00'),
      findsOneWidget,
    );
    expect(find.text('Activa'), findsOneWidget);
    expect(find.textContaining('Próxima: '), findsOneWidget);
    expect(find.text('texto del encargo que no se muestra'), findsNothing);
  });

  testWidgets('the switch pauses an active task', (tester) async {
    final host = ScriptedTransport({
      'GET /v1/schedules': _list([_schedule()]),
      'POST /v1/schedules/sch_1/pause': jsonEncode({
        'ok': true,
        'schedule': _schedule(status: 'paused', nextRunAt: null),
      }),
    });
    await pumpSection(tester, host);

    await tester.tap(find.byType(Switch));
    await tester.pumpAndSettle();

    expect(host.calls, contains('POST /v1/schedules/sch_1/pause'));
    expect(find.text('En pausa'), findsOneWidget);
  });

  testWidgets('«Lanzar ahora» asks first, then says it launched', (
    tester,
  ) async {
    final host = ScriptedTransport({
      'GET /v1/schedules': _list([_schedule()]),
      'POST /v1/schedules/sch_1/run_now':
          '{"ok":true,"delegation_id":"del_1","status":"queued","depth":1}',
    });
    await pumpSection(tester, host);

    await tester.tap(find.text('Lanzar ahora'));
    await tester.pumpAndSettle();
    expect(host.calls, isNot(contains('POST /v1/schedules/sch_1/run_now')));

    await tester.tap(find.text('Lanzar'));
    await tester.pumpAndSettle();

    expect(host.calls, contains('POST /v1/schedules/sch_1/run_now'));
    expect(find.text('Encargo lanzado'), findsOneWidget);
  });

  testWidgets('a failed read says so and «Reintentar» reads again', (
    tester,
  ) async {
    final host = ScriptedTransport({'GET /v1/schedules': _list([])});
    host.failNext('GET /v1/schedules', HostError(500, ''));
    await pumpSection(tester, host);

    expect(
      find.text(
        'No se pudieron leer las tareas programadas. Inténtalo de nuevo.',
      ),
      findsOneWidget,
    );

    await tester.tap(find.text('Reintentar'));
    await tester.pumpAndSettle();

    expect(host.calls, hasLength(2));
  });

  testWidgets('without APPROVE the switch is off and the device is told why', (
    tester,
  ) async {
    final host = ScriptedTransport({
      'GET /v1/schedules': _list([_schedule()]),
    });
    host.failNext('POST /v1/schedules/sch_1/pause', HostError(403, ''));
    await pumpSection(tester, host);

    await tester.tap(find.byType(Switch));
    await tester.pumpAndSettle();

    expect(find.textContaining('Concede el permiso APPROVE'), findsOneWidget);
    expect(tester.widget<Switch>(find.byType(Switch)).onChanged, isNull);
  });

  testWidgets('the history lists each outcome and never the text of the task', (
    tester,
  ) async {
    final host = ScriptedTransport({
      'GET /v1/schedules': _list([_schedule()]),
      'GET /v1/schedules/sch_1/runs': jsonEncode({
        'ok': true,
        'runs': [
          {
            'id': 'srn_1',
            'schedule_id': 'sch_1',
            'due_at': '2026-11-02T08:00:00.000Z',
            'started_at': null,
            'outcome': 'missed_offline',
            'delegation_id': null,
          },
        ],
      }),
    });
    await pumpSection(tester, host);

    await tester.tap(find.text('Historial'));
    await tester.pumpAndSettle();

    expect(find.text('Perdida: tu PC estaba apagado'), findsOneWidget);
    expect(find.text('texto del encargo que no se muestra'), findsNothing);
  });

  for (final (width, scale) in [(360.0, 1.0), (360.0, 1.3)]) {
    testWidgets('no overflow at $width dp with text ${scale}x', (tester) async {
      final host = ScriptedTransport({
        'GET /v1/schedules': _list([_schedule()]),
      });

      await pumpSection(tester, host, width: width, textScale: scale);

      expect(tester.takeException(), isNull);
    });
  }
}
