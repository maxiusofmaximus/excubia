import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/chat_transport.dart';
import 'package:excubia/core/api/connection_failure.dart';
import 'package:excubia/core/api/connection_text.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/restore_api.dart';
import 'package:excubia/ui/restore/restore_screen.dart';
import 'package:excubia/ui/restore/restore_text.dart';
import 'package:excubia/ui/theme.dart';

const String _agent = 'w1:p1';
const String _pointsRoute = 'GET /v1/restore/points?agent=w1%3Ap1';
const String _filesRoute = 'GET /v1/restore/points/01A/files';
const String _restoreRoute = 'POST /v1/restore/points/01A/restore';

/// Answers each route in order and repeats the last answer. A [HostError]
/// answer is thrown the way the real client refuses.
class _Host implements ChatTransport {
  _Host(this._answers);

  final Map<String, List<Object>> _answers;
  final List<String> calls = [];

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    final route = '$method $target';
    calls.add(route);
    final queue = _answers[route];
    if (queue == null) throw const HostError(404, '{"detail":"not found"}');
    final answer = queue.length > 1 ? queue.removeAt(0) : queue.first;
    if (answer is HostError) throw answer;
    return answer as String;
  }

  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError();

  @override
  Future<List<int>> callBytes(String method, String target) =>
      throw UnimplementedError();
}

String _ok(Map<String, Object?> fields) => jsonEncode({'ok': true, ...fields});

String _points(List<Map<String, Object?>> points) =>
    _ok({'agent': _agent, 'points': points});

Map<String, Object?> _point({
  String id = '01A',
  String kind = 'turn',
  required int createdMs,
  int files = 1,
  int excluded = 0,
}) => {
  'id': id,
  'kind': kind,
  'turn': '01T',
  'created_ms': createdMs,
  'files': files,
  'excluded': excluded,
};

int get _nowMs => DateTime.now().millisecondsSinceEpoch;

String _changes(List<Map<String, Object?>> changes, [List<Map<String, Object?>> excluded = const []]) =>
    _ok({'point': '01A', 'changes': changes, 'excluded': excluded});

final String _threeChanges = _changes([
  {'path': 'notas/nueva.md', 'status': 'added'},
  {'path': 'notas/cambio.md', 'status': 'modified'},
  {'path': 'notas/vieja.md', 'status': 'deleted'},
], [
  {'path': 'node_modules', 'reason': 'symlink'},
  {'path': 'bin', 'reason': 'special'},
]);

Future<void> _mount(
  WidgetTester tester,
  _Host host, {
  Size size = const Size(800, 3200),
  double textScale = 1,
}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(
          context,
        ).copyWith(textScaler: TextScaler.linear(textScale)),
        child: child!,
      ),
      home: RestoreScreen(
        api: RestoreApi(host),
        agent: _agent,
        title: 'proyecto-x',
      ),
    ),
  );
  await tester.pumpAndSettle();
}

/// Opens the first copy of the list and waits for its changes.
Future<void> _openFirstPoint(WidgetTester tester) async {
  await tester.tap(find.byType(ListTile).first);
  await tester.pumpAndSettle();
}

/// Goes from the detail of the first copy to its confirmation.
Future<void> _openConfirmation(WidgetTester tester) async {
  await _openFirstPoint(tester);
  await tester.tap(find.widgetWithText(FilledButton, 'Volver a este punto'));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('lists the copies with the newest first', (tester) async {
    final host = _Host({
      _pointsRoute: [
        _points([
          _point(id: '01OLD', createdMs: _nowMs - 3 * 3600000),
          _point(id: '01NEW', createdMs: _nowMs - 5 * 60000),
        ]),
      ],
    });

    await _mount(tester, host);

    final newest = tester.getTopLeft(find.text('hace 5 min · 1 fichero'));
    final oldest = tester.getTopLeft(find.text('hace 3 h · 1 fichero'));
    expect(newest.dy, lessThan(oldest.dy));
  });

  testWidgets('says there are no copies yet and reads again on request', (
    tester,
  ) async {
    final host = _Host({
      _pointsRoute: [_points([])],
    });

    await _mount(tester, host);

    expect(
      find.text(
        'Aún no hay copias de este agente. Se guarda una al empezar cada turno.',
      ),
      findsOneWidget,
    );
    await tester.tap(find.text('Actualizar'));
    await tester.pumpAndSettle();
    expect(host.calls.where((call) => call == _pointsRoute).length, 2);
  });

  testWidgets('shows a failed read with Reintentar, and reads again', (
    tester,
  ) async {
    final host = _Host({
      _pointsRoute: [
        const HostError.transport(ConnectionFailure.noRoute, 'socket'),
        _points([_point(createdMs: _nowMs - 60000)]),
      ],
    });

    await _mount(tester, host);
    expect(
      find.text(connectionText(ConnectionFailure.noRoute)),
      findsOneWidget,
    );

    await tester.tap(find.text('Reintentar'));
    await tester.pumpAndSettle();
    expect(find.text('hace 1 min · 1 fichero'), findsOneWidget);
  });

  testWidgets('waits for the grant instead of an error when the device cannot read yet', (
    tester,
  ) async {
    final host = _Host({
      _pointsRoute: [const HostError(403, '{"detail":"forbidden"}')],
    });

    await _mount(tester, host);

    expect(find.text('Esperando que aprueben este dispositivo.'), findsOneWidget);
    expect(find.text('Reintentar'), findsNothing);
  });

  testWidgets('shows what a copy would change and what was left out', (
    tester,
  ) async {
    final host = _Host({
      _pointsRoute: [_points([_point(createdMs: _nowMs - 60000)])],
      _filesRoute: [_threeChanges],
    });

    await _mount(tester, host);
    await _openFirstPoint(tester);

    expect(find.text('nuevo'), findsOneWidget);
    expect(find.text('cambiado'), findsOneWidget);
    expect(find.text('borrado'), findsOneWidget);
    expect(find.text('notas/nueva.md'), findsOneWidget);
    expect(
      find.text(
        'No se guardaron 2 elementos: enlaces y elementos especiales.',
      ),
      findsOneWidget,
    );
  });

  testWidgets('asks before restoring and says what it will change', (
    tester,
  ) async {
    final host = _Host({
      _pointsRoute: [_points([_point(createdMs: _nowMs - 60000)])],
      _filesRoute: [_threeChanges],
    });

    await _mount(tester, host);
    await _openConfirmation(tester);

    expect(
      find.text(
        'Se van a restaurar 3 ficheros. Antes se guarda una copia de la carpeta actual para poder deshacer.',
      ),
      findsOneWidget,
    );
    expect(find.text(restoreWarning), findsOneWidget);
    expect(host.calls, isNot(contains(_restoreRoute)));
  });

  testWidgets('shows the grant note when the device cannot restore', (
    tester,
  ) async {
    final host = _Host({
      _pointsRoute: [_points([_point(createdMs: _nowMs - 60000)])],
      _filesRoute: [_threeChanges],
      _restoreRoute: [const HostError(403, '{"detail":"forbidden"}')],
    });

    await _mount(tester, host);
    await _openConfirmation(tester);
    await tester.tap(find.widgetWithText(FilledButton, 'Volver a este punto'));
    await tester.pumpAndSettle();

    expect(find.text(restoreGrantLede), findsOneWidget);
    expect(find.text('Hecho.'), findsNothing);
  });

  testWidgets('says what came back, and undo restores the copy kept before', (
    tester,
  ) async {
    final host = _Host({
      _pointsRoute: [_points([_point(createdMs: _nowMs - 60000)])],
      _filesRoute: [_threeChanges],
      _restoreRoute: [
        _ok({
          'point': '01A',
          'restored': 2,
          'removed': 1,
          'undo_point': '01U',
        }),
      ],
      'POST /v1/restore/points/01U/restore': [
        _ok({'point': '01U', 'restored': 3, 'removed': 0, 'undo_point': '01V'}),
      ],
    });

    await _mount(tester, host);
    await _openConfirmation(tester);
    await tester.tap(find.widgetWithText(FilledButton, 'Volver a este punto'));
    await tester.pumpAndSettle();

    expect(find.text('Hecho.'), findsOneWidget);
    expect(
      find.text('Se han restaurado 2 ficheros. Se han quitado 1 fichero.'),
      findsOneWidget,
    );
    await tester.tap(find.text('Deshacer'));
    await tester.pumpAndSettle();

    expect(find.text('Deshecho.'), findsOneWidget);
    expect(host.calls, contains('POST /v1/restore/points/01U/restore'));
  });

  testWidgets('keeps a long path and large text inside a 360 dp phone', (
    tester,
  ) async {
    final host = _Host({
      _pointsRoute: [_points([_point(createdMs: _nowMs - 60000)])],
      _filesRoute: [
        _changes([
          {
            'path': 'proyectos/cliente-alfa/documentacion/revisiones/2026/'
                'informe-final-revisado-definitivo-v3-copia.md',
            'status': 'modified',
          },
        ]),
      ],
    });

    await _mount(tester, host, size: const Size(360, 800), textScale: 1.3);
    expect(tester.takeException(), isNull);
    await _openConfirmation(tester);

    expect(tester.takeException(), isNull);
  });
}
