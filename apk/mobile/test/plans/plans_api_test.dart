import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/connection_failure.dart';
import 'package:excubia/core/api/connection_text.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/plan_models.dart';
import 'package:excubia/core/api/plans_api.dart';

import '../support/scripted_transport.dart';

const _planJson =
    '{"id":"plan_1","status":"approved","steps":[],"summary":"Plan de 0 pasos",'
    '"created_at":"2026-11-05T10:00:00.000Z"}';

String _answer(String plan) => '{"ok":true,"plan":$plan}';

void main() {
  group('reading the plans', () {
    test('one GET of the plans, parsed in the host order', () async {
      final host = ScriptedTransport({
        'GET /v1/plans': '{"ok":true,"plans":[$_planJson]}',
      });

      final plans = await PlansApi(host).plans();

      expect(host.calls, ['GET /v1/plans']);
      expect(plans.single.id, 'plan_1');
    });
  });

  group('answering a plan', () {
    test('approve and reject send an empty JSON object to their own route', () async {
      final host = ScriptedTransport({
        'POST /v1/plans/plan_1/approve': _answer(_planJson),
        'POST /v1/plans/plan_1/reject': _answer(_planJson),
      });

      final approved = await PlansApi(host).answer('plan_1', PlanDecision.approve);
      await PlansApi(host).answer('plan_1', PlanDecision.reject);

      expect(approved.status, PlanStatus.approved);
      expect(host.calls, [
        'POST /v1/plans/plan_1/approve',
        'POST /v1/plans/plan_1/reject',
      ]);
      expect(host.bodies['POST /v1/plans/plan_1/approve'], [{}]);
      expect(host.bodies['POST /v1/plans/plan_1/reject'], [{}]);
    });

    test('request changes sends the note in its body', () async {
      final host = ScriptedTransport({
        'POST /v1/plans/plan_1/request_changes': _answer(_planJson),
      });

      await PlansApi(host).answer(
        'plan_1',
        PlanDecision.requestChanges,
        note: 'Cambia el paso 2',
      );

      expect(host.calls, ['POST /v1/plans/plan_1/request_changes']);
      expect(host.bodies['POST /v1/plans/plan_1/request_changes'], [
        {'note': 'Cambia el paso 2'},
      ]);
    });

    test('request changes without a note is refused before the wire', () {
      final host = ScriptedTransport({});

      expect(
        PlansApi(host).answer('plan_1', PlanDecision.requestChanges),
        throwsArgumentError,
      );
      expect(host.calls, isEmpty);
    });

    test('an id is kept in its own path segment', () async {
      final host = ScriptedTransport({
        'POST /v1/plans/a%2Fb/approve': _answer(_planJson),
      });

      await PlansApi(host).answer('a/b', PlanDecision.approve);

      expect(host.calls, ['POST /v1/plans/a%2Fb/approve']);
    });
  });

  group('the words of a failed decision', () {
    String failure(int status, String code) => planDecisionFailureText(
      HostError(status, jsonEncode({'detail': {'code': code}})),
    );

    test('a plan decided somewhere else says so', () {
      expect(
        failure(409, 'plan_decided'),
        'Este plan ya se decidió desde otro dispositivo.',
      );
    });

    test('a plan not yet approved, and a plan that is gone', () {
      expect(
        failure(409, 'plan_not_approved'),
        'Este plan todavía no está aprobado.',
      );
      expect(
        failure(404, 'no_such_plan'),
        'Ese plan ya no existe. Se quitará de la lista.',
      );
    });

    test('a refused note has words of its own', () {
      expect(
        failure(400, 'invalid_body'),
        'La nota no es válida. Revísala y vuelve a enviarla.',
      );
    });

    test('a refusal for lack of APPROVE says how to grant it', () {
      expect(planDecisionFailureText(const HostError(403, '{}')), planPermissionText);
    });

    test('a link failure keeps the link words', () {
      expect(
        planDecisionFailureText(
          const HostError.transport(ConnectionFailure.hostDown, ''),
        ),
        connectionText(ConnectionFailure.hostDown),
      );
    });

    test('any other failure says the action did not finish', () {
      expect(
        planDecisionFailureText(StateError('x')),
        'No se pudo completar la acción. Inténtalo de nuevo.',
      );
    });
  });

  group('the words of a failed read', () {
    test('a 403 is the wait for the grant, not an error', () {
      expect(
        planReadFailureText(const HostError(403, '{}')),
        'Esperando que aprueben este dispositivo.',
      );
    });

    test('any other failure says the plans could not be read', () {
      expect(
        planReadFailureText(const HostError(500, '{}')),
        'No se pudo leer los planes. Inténtalo de nuevo.',
      );
    });
  });
}
