import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/connection_failure.dart';
import 'package:herdr_notify/core/api/connection_text.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/api/recipes_api.dart';
import 'package:herdr_notify/core/api/read_grant.dart';

import '../support/scripted_transport.dart';

const _list =
    '{"ok":true,"recipes":[{"id":"rcp_1","role_id":"audio-editor",'
    '"title":"Resumen","body":"Haz un resumen.","uses":2,'
    '"created_at":"2026-11-01T10:00:00.000Z","updated_at":"2026-11-02T10:00:00.000Z"}]}';

const _run = '{"ok":true,"delegation_id":"del_1","status":"queued","depth":1}';

String _coded(String code) => jsonEncode({
  'detail': {'code': code},
});

void main() {
  group('reading the recipes of one oficio', () {
    test('one GET that names the role in its query', () async {
      final host = ScriptedTransport({
        'GET /v1/recipes?role=audio-editor': _list,
      });

      final recipes = await RecipesApi(host).recipes('audio-editor');

      expect(host.calls, ['GET /v1/recipes']);
      expect(recipes.single.title, 'Resumen');
    });

    test('a refused read is a 403 that waits for the grant', () {
      const error = HostError(403, '{}');

      expect(recipeReadWaitingText(error), readGrantWaitingText);
    });

    test('a transport failure keeps its own words', () {
      const error = HostError.transport(ConnectionFailure.noRoute, '');

      expect(
        recipeReadFailureText(error),
        connectionText(ConnectionFailure.noRoute),
      );
    });

    test('any other read failure says it could not be read', () {
      expect(
        recipeReadFailureText(const HostError(500, '{}')),
        'No se pudieron leer las recetas. Inténtalo de nuevo.',
      );
    });
  });

  group('running a recipe', () {
    test('POST to its run route, with the conversation in the body', () async {
      final host = ScriptedTransport({'POST /v1/recipes/rcp_1/run': _run});

      final run = await RecipesApi(host).run('rcp_1', conversationId: 'conv_1');

      expect(run.delegationId, 'del_1');
      expect(run.status, 'queued');
      expect(host.bodies['POST /v1/recipes/rcp_1/run'], [
        {'conversation_id': 'conv_1'},
      ]);
    });
  });

  group('the words of a refused run', () {
    test('a busy agent says how long to wait', () {
      const error = HostError(
        409,
        '{"detail":{"code":"agent_busy"}}',
        retryAfter: 12,
      );

      expect(
        recipeRunFailureText(error),
        'Ahora está ocupado; vuelve a intentarlo en 12 s.',
      );
    });

    test('a busy agent without Retry-After gives no number', () {
      expect(
        recipeRunFailureText(HostError(409, _coded('agent_busy'))),
        'Ahora está ocupado; vuelve a intentarlo en un momento.',
      );
    });

    test('a missing hire is said with the way out', () {
      final error = HostError(409, _coded('no_hire_for_role'));

      expect(
        recipeRunFailureText(error),
        'Ya no tienes a nadie contratado para este oficio. Contrata a alguien para repetirla.',
      );
      expect(recipeNeedsHire(error), isTrue);
    });

    test('only a missing hire offers the hire action', () {
      expect(recipeNeedsHire(HostError(409, _coded('agent_busy'))), isFalse);
    });

    test('a conversation without a project says what to open', () {
      expect(
        recipeRunFailureText(
          HostError(409, _coded('conversation_has_no_project')),
        ),
        noProjectRunText,
      );
      expect(recipeRunFailureText(const RecipeNoProject()), noProjectRunText);
    });

    test('a plan that is not approved yet is said plainly', () {
      expect(
        recipeRunFailureText(HostError(409, _coded('plan_not_approved'))),
        'El plan de esa conversación todavía no está aprobado.',
      );
    });

    test('a 403 says the device only sees the recipes', () {
      expect(
        recipeRunFailureText(const HostError(403, '{}')),
        recipePermissionText,
      );
    });

    test('a refusal that is not ours says the run did not start', () {
      expect(
        recipeRunFailureText(HostError(500, '{}')),
        'No se pudo lanzar el encargo. Inténtalo de nuevo.',
      );
    });

    test('the raw host words never reach the screen', () {
      final text = recipeRunFailureText(
        const HostError(
          409,
          '{"detail":{"code":"x","message":"secret_token_123"}}',
        ),
      );

      expect(text, isNot(contains('secret_token_123')));
    });
  });
}
