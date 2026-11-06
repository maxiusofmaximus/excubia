import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/api/recipes_api.dart';
import 'package:herdr_notify/core/state/recipes_controller.dart';

import '../support/scripted_transport.dart';

const _listKey = 'GET /v1/recipes?role=audio-editor';
const _listPath = 'GET /v1/recipes';
const _runKey = 'POST /v1/recipes/rcp_1/run';

const _list =
    '{"ok":true,"recipes":[{"id":"rcp_1","role_id":"audio-editor",'
    '"title":"Resumen","body":"Haz un resumen.","uses":2,'
    '"created_at":"2026-11-01T10:00:00.000Z","updated_at":"2026-11-02T10:00:00.000Z"}]}';

const _run = '{"ok":true,"delegation_id":"del_1","status":"queued","depth":1}';

ScriptedTransport _host() =>
    ScriptedTransport({_listKey: _list, _runKey: _run});

RecipesController _controller(
  ScriptedTransport host, {
  String? conversation = 'conv_1',
}) => RecipesController(
  api: RecipesApi(host),
  conversationId: () => conversation,
);

void main() {
  group('opening the list of an oficio', () {
    test('reads its recipes and holds them', () async {
      final host = _host();
      final ctrl = _controller(host);

      await ctrl.open('audio-editor');

      expect(ctrl.loaded, isTrue);
      expect(ctrl.readError, isNull);
      expect(ctrl.recipes.single.id, 'rcp_1');
    });

    test('a refused read is kept for the screen, not thrown', () async {
      final host = _host()..failNext(_listPath, const HostError(403, '{}'));
      final ctrl = _controller(host);

      await ctrl.open('audio-editor');

      expect(ctrl.loaded, isFalse);
      expect(ctrl.readError, isA<HostError>());
      expect(ctrl.recipes, isEmpty);
    });
  });

  group('repeating a recipe', () {
    test('runs it in the open conversation and reads the list again', () async {
      final host = _host();
      final ctrl = _controller(host);
      await ctrl.open('audio-editor');

      final started = await ctrl.run(ctrl.recipes.single);

      expect(started, isTrue);
      expect(ctrl.lastRun?.delegationId, 'del_1');
      expect(ctrl.runError, isNull);
      expect(host.bodies[_runKey], [
        {'conversation_id': 'conv_1'},
      ]);
      expect(host.calls.where((call) => call == 'GET /v1/recipes').length, 2);
    });

    test('with no conversation that has a project, nothing is sent', () async {
      final host = _host();
      final ctrl = _controller(host, conversation: null);
      await ctrl.open('audio-editor');

      final started = await ctrl.run(ctrl.recipes.single);

      expect(started, isFalse);
      expect(host.calls, [_listPath]);
      expect(ctrl.runError, isA<RecipeNoProject>());
    });

    test('a 403 locks the runs until the list is opened again', () async {
      final host = _host()..failNext(_runKey, const HostError(403, '{}'));
      final ctrl = _controller(host);
      await ctrl.open('audio-editor');

      expect(await ctrl.run(ctrl.recipes.single), isFalse);
      expect(ctrl.denied, isTrue);
      expect(await ctrl.run(ctrl.recipes.single), isFalse);
      expect(host.calls.where((call) => call == _runKey).length, 1);

      await ctrl.open('audio-editor');
      expect(ctrl.denied, isFalse);
    });

    testWidgets('a busy agent counts down to zero, then the run is allowed', (
      tester,
    ) async {
      final host = _host()
        ..failNext(
          _runKey,
          HostError(
            409,
            jsonEncode({
              'detail': {'code': 'agent_busy'},
            }),
            retryAfter: 2,
          ),
        );
      final ctrl = _controller(host);
      await ctrl.open('audio-editor');

      expect(await ctrl.run(ctrl.recipes.single), isFalse);
      expect(ctrl.retryIn, 2);

      await tester.pump(const Duration(seconds: 1));
      expect(ctrl.retryIn, 1);
      await tester.pump(const Duration(seconds: 1));
      expect(ctrl.retryIn, 0);

      expect(await ctrl.run(ctrl.recipes.single), isTrue);
      ctrl.dispose();
    });

    testWidgets('disposing during a countdown stops the timer', (tester) async {
      final host = _host()
        ..failNext(
          _runKey,
          const HostError(
            409,
            '{"detail":{"code":"agent_busy"}}',
            retryAfter: 5,
          ),
        );
      final ctrl = _controller(host);
      await ctrl.open('audio-editor');
      await ctrl.run(ctrl.recipes.single);

      ctrl.dispose();
      await tester.pump(const Duration(seconds: 6));

      expect(tester.takeException(), isNull);
    });
  });
}
