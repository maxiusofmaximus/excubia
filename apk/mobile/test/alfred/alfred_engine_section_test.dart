import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/alfred_backend_api.dart';
import 'package:herdr_notify/core/api/chat_transport.dart';
import 'package:herdr_notify/core/api/connection_failure.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/alfred/alfred_engine_section.dart';

import '../support/widget_mount.dart';

const String _read = 'GET /v1/alfred/backend';
const String _change = 'POST /v1/alfred/backend';

String _state(String state, {String? detail}) =>
    '{"backend":"${state == 'local' || state == 'starting' ? 'local' : 'claude'}",'
    '"state":"$state","detail":${detail == null ? 'null' : '"$detail"'},'
    '"since":1,"voice_suspended":false}';

/// Answers by route. Each route has a queue of answers: a `String` is a 200 body, a
/// `Future<String>` is an answer still pending, a `HostError` is thrown; the last one repeats.
class _Engine implements ChatTransport {
  _Engine(this.answers);

  final Map<String, List<Object>> answers;
  final List<String> calls = [];
  final List<Map<String, Object?>> sent = [];

  int count(String key) => calls.where((call) => call == key).length;

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    final key = '$method $target';
    calls.add(key);
    if (body.isNotEmpty) sent.add({'route': key, 'body': body.length});
    final queue = answers[key]!;
    final answer = queue.length > 1 ? queue.removeAt(0) : queue.first;
    if (answer is HostError) throw answer;
    if (answer is Future<String>) return answer;
    return answer as String;
  }

  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError();

  @override
  Future<List<int>> callBytes(String method, String target) =>
      throw UnimplementedError();
}

Future<void> _mount(WidgetTester tester, _Engine engine) => mountWidget(
  tester,
  Scaffold(
    body: ListView(
      padding: const EdgeInsets.all(16),
      children: [AlfredEngineSection(api: AlfredBackendApi(engine))],
    ),
  ),
);

Finder _button(String label) => find.widgetWithText(OutlinedButton, label);

OutlinedButton _buttonOf(WidgetTester tester, String label) =>
    tester.widget<OutlinedButton>(_button(label));

void main() {
  setUpAll(() async => RustLib.init());

  testWidgets('Claude offers the local model and asks before changing', (
    tester,
  ) async {
    final engine = _Engine({
      _read: [_state('claude')],
    });
    await _mount(tester, engine);

    expect(find.text('Claude'), findsOneWidget);
    expect(_buttonOf(tester, 'Usar el modelo local').onPressed, isNotNull);

    await tester.tap(_button('Usar el modelo local'));
    await tester.pumpAndSettle();
    expect(find.textContaining('34 GB de RAM'), findsOneWidget);
    expect(find.textContaining('unos 4 minutos'), findsOneWidget);
    expect(engine.count(_change), 0);
  });

  testWidgets('cancelling the confirmation does not post the change', (
    tester,
  ) async {
    final engine = _Engine({
      _read: [_state('claude')],
    });
    await _mount(tester, engine);

    await tester.tap(_button('Usar el modelo local'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Cancelar'));
    await tester.pumpAndSettle();

    expect(engine.count(_change), 0);
    expect(find.text('Claude'), findsOneWidget);
  });

  testWidgets(
    'confirming posts once and shows the start, disabled, until local',
    (tester) async {
      final engine = _Engine({
        _read: [_state('claude')],
        _change: [_state('starting')],
      });
      await _mount(tester, engine);

      await tester.tap(_button('Usar el modelo local'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Cambiar'));
      await tester.pumpAndSettle();

      expect(engine.count(_change), 1);
      expect(
        find.text('Arrancando el modelo local… (unos 4 min)'),
        findsOneWidget,
      );
      expect(_buttonOf(tester, 'Usar el modelo local').onPressed, isNull);
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets('a local model goes back to Claude without asking', (
    tester,
  ) async {
    final engine = _Engine({
      _read: [_state('local')],
      _change: [_state('stopping')],
    });
    await _mount(tester, engine);

    expect(find.text('Modelo local'), findsOneWidget);
    await tester.tap(_button('Volver a Claude'));
    await tester.pumpAndSettle();

    expect(find.byType(AlertDialog), findsNothing);
    expect(engine.sent.single['route'], _change);
    expect(find.text('Volviendo a Claude…'), findsOneWidget);
    expect(_buttonOf(tester, 'Volver a Claude').onPressed, isNull);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('polls every 3 seconds until local, then stops', (tester) async {
    final engine = _Engine({
      _read: [_state('starting'), _state('starting'), _state('local')],
    });
    await _mount(tester, engine);
    expect(
      find.text('Arrancando el modelo local… (unos 4 min)'),
      findsOneWidget,
    );

    await tester.pump(const Duration(seconds: 3));
    await tester.pump(const Duration(seconds: 3));
    expect(engine.count(_read), 3);
    expect(find.text('Modelo local'), findsOneWidget);

    await tester.pump(const Duration(seconds: 9));
    expect(engine.count(_read), 3);
  });

  testWidgets('a poll does not start while the last read is still pending', (
    tester,
  ) async {
    final pending = Completer<String>();
    final engine = _Engine({
      _read: [_state('starting'), pending.future],
    });
    await _mount(tester, engine);

    await tester.pump(const Duration(seconds: 3));
    await tester.pump(const Duration(seconds: 3));
    expect(engine.count(_read), 2);

    pending.complete(_state('local'));
    await tester.pumpAndSettle();
    expect(find.text('Modelo local'), findsOneWidget);
  });

  testWidgets('leaving the screen cancels the polling', (tester) async {
    final engine = _Engine({
      _read: [_state('starting')],
    });
    await _mount(tester, engine);

    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(seconds: 9));
    expect(engine.count(_read), 1);
  });

  testWidgets('a 403 says so once and is not retried', (tester) async {
    final engine = _Engine({
      _read: [_state('claude')],
      _change: [const HostError(403, '{"detail":"forbidden"}')],
    });
    await _mount(tester, engine);

    await tester.tap(_button('Usar el modelo local'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Cambiar'));
    await tester.pumpAndSettle();
    await tester.pump(const Duration(seconds: 9));

    expect(
      find.text('Este móvil no puede cambiar el motor de Alfred.'),
      findsOneWidget,
    );
    expect(engine.count(_change), 1);
  });

  testWidgets('a PC without the setting greys the section out', (tester) async {
    final engine = _Engine({
      _read: [const HostError(404, '{"detail":"not found"}')],
    });
    await _mount(tester, engine);

    expect(
      find.text('Tu PC no admite este ajuste; actualiza Excubia en el PC.'),
      findsOneWidget,
    );
    expect(_button('Usar el modelo local'), findsNothing);
    final opacity = tester.widget<Opacity>(
      find
          .ancestor(
            of: find.text(
              'Tu PC no admite este ajuste; actualiza Excubia en el PC.',
            ),
            matching: find.byType(Opacity),
          )
          .first,
    );
    expect(opacity.opacity, lessThan(1));
  });

  testWidgets('a busy PC says so and shows what it is doing', (tester) async {
    final engine = _Engine({
      _read: [_state('claude'), _state('starting')],
      _change: [const HostError(409, '{"detail":{"code":"backend_busy"}}')],
    });
    await _mount(tester, engine);

    await tester.tap(_button('Usar el modelo local'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Cambiar'));
    await tester.pumpAndSettle();

    expect(find.text('Alfred ya está cambiando de modelo.'), findsOneWidget);
    expect(
      find.text('Arrancando el modelo local… (unos 4 min)'),
      findsOneWidget,
    );
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('a lost link on the first read offers to try again', (
    tester,
  ) async {
    final engine = _Engine({
      _read: [
        const HostError.transport(ConnectionFailure.noRoute, ''),
        _state('claude'),
      ],
    });
    await _mount(tester, engine);

    expect(find.text('No se pudo leer'), findsOneWidget);
    await tester.tap(find.text('Reintentar'));
    await tester.pumpAndSettle();

    expect(find.text('Claude'), findsOneWidget);
    expect(find.text('No se pudo leer'), findsNothing);
  });

  testWidgets('a failed start says why and allows the local model again', (
    tester,
  ) async {
    final engine = _Engine({
      _read: [_state('failed', detail: 'Sin RAM libre para el modelo.')],
    });
    await _mount(tester, engine);

    expect(
      find.text('No pude cambiar: Sin RAM libre para el modelo.'),
      findsOneWidget,
    );
    expect(_buttonOf(tester, 'Usar el modelo local').onPressed, isNotNull);
  });

  testWidgets(
    'a failure with the turns still on the local model offers the way back',
    (tester) async {
      final engine = _Engine({
        _read: [
          '{"backend":"local","state":"failed",'
              '"detail":"Alfred usa el modelo local, pero Strata no está en marcha",'
              '"since":1,"voice_suspended":false}',
        ],
        _change: [_state('stopping')],
      });
      await _mount(tester, engine);

      expect(find.text('Usar el modelo local'), findsNothing);
      await tester.tap(_button('Volver a Claude'));
      await tester.pumpAndSettle();

      expect(find.byType(AlertDialog), findsNothing);
      expect(engine.sent.single['route'], _change);
      await tester.pumpWidget(const SizedBox());
    },
  );
}
