import 'dart:async';
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/plan_models.dart';
import 'package:excubia/core/api/plans_api.dart';
import 'package:excubia/core/state/plans_controller.dart';

import '../support/scripted_transport.dart';

String _plan(String id, String status, {String? updatedAt}) => jsonEncode({
  'id': id,
  'status': status,
  'steps': [
    {
      'title': 'Leer el repo',
      'touches': ['files'],
    },
  ],
  'summary': 'Plan de 1 paso · lo propone Ernesto',
  'created_at': '2026-11-05T10:00:00.000Z',
  'updated_at': ?updatedAt,
});

String _list(List<String> plans) => '{"ok":true,"plans":[${plans.join(',')}]}';

String _answer(String plan) => '{"ok":true,"plan":$plan}';

/// Holds every call until [release] is called, so an overlap can be tried.
class _GatedTransport extends ScriptedTransport {
  _GatedTransport(super.routes);

  final Completer<void> gate = Completer<void>();

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    await gate.future;
    return super.call(method, target, body);
  }
}

PlansController _controller(
  ScriptedTransport host, {
  Duration interval = const Duration(hours: 1),
}) => PlansController(api: PlansApi(host), interval: interval);

void main() {
  test('a refresh reads the plans and says the first answer came', () async {
    final host = ScriptedTransport({
      'GET /v1/plans': _list([_plan('plan_a', 'proposed')]),
    });
    final plans = _controller(host);
    addTearDown(plans.dispose);

    expect(plans.loaded, isFalse);
    await plans.refresh();

    expect(plans.loaded, isTrue);
    expect(plans.plans.single.id, 'plan_a');
    expect(plans.pendingCount, 1);
    expect(plans.error, isNull);
  });

  test('a failed read keeps the last plans and keeps the failure', () async {
    final host = ScriptedTransport({
      'GET /v1/plans': _list([_plan('plan_a', 'proposed')]),
    });
    final plans = _controller(host);
    addTearDown(plans.dispose);
    await plans.refresh();

    host.failNext('GET /v1/plans', const HostError(500, '{}'));
    await plans.refresh();

    expect(plans.plans.single.id, 'plan_a');
    expect(plans.error, isA<HostError>());
  });

  test('polls only while shown, and stops when hidden', () async {
    final host = ScriptedTransport({'GET /v1/plans': _list([])});
    final plans = _controller(host, interval: const Duration(milliseconds: 5));
    addTearDown(plans.dispose);

    plans.setActive(true);
    await Future<void>.delayed(const Duration(milliseconds: 40));
    plans.setActive(false);
    final reads = host.calls.length;
    expect(reads, greaterThan(1));

    await Future<void>.delayed(const Duration(milliseconds: 40));
    expect(host.calls.length, reads);
  });

  test('approving replaces the plan with the host answer', () async {
    final host = ScriptedTransport({
      'GET /v1/plans': _list([_plan('plan_a', 'proposed')]),
      'POST /v1/plans/plan_a/approve': _answer(_plan('plan_a', 'approved')),
    });
    final plans = _controller(host);
    addTearDown(plans.dispose);
    await plans.refresh();

    final done = await plans.decide(plans.plans.single, PlanDecision.approve);

    expect(done, isTrue);
    expect(plans.plans.single.status, PlanStatus.approved);
    expect(plans.pendingCount, 0);
    expect(plans.isBusy('plan_a'), isFalse);
  });

  test('requesting changes sends the note with the decision', () async {
    final host = ScriptedTransport({
      'GET /v1/plans': _list([_plan('plan_a', 'proposed')]),
      'POST /v1/plans/plan_a/request_changes': _answer(
        _plan('plan_a', 'changes_requested'),
      ),
    });
    final plans = _controller(host);
    addTearDown(plans.dispose);
    await plans.refresh();

    await plans.decide(
      plans.plans.single,
      PlanDecision.requestChanges,
      note: 'Cambia el paso 2',
    );

    expect(host.bodies['POST /v1/plans/plan_a/request_changes'], [
      {'note': 'Cambia el paso 2'},
    ]);
    expect(plans.plans.single.status, PlanStatus.changesRequested);
  });

  test('a second decision while one is in flight is not sent', () async {
    final host = _GatedTransport({
      'GET /v1/plans': _list([_plan('plan_a', 'proposed')]),
      'POST /v1/plans/plan_a/reject': _answer(_plan('plan_a', 'rejected')),
    });
    final plans = _controller(host);
    addTearDown(plans.dispose);
    host.gate.complete();
    await plans.refresh();
    final plan = plans.plans.single;

    final first = plans.decide(plan, PlanDecision.reject);
    expect(plans.isBusy('plan_a'), isTrue);
    final second = await plans.decide(plan, PlanDecision.reject);
    await first;

    expect(second, isFalse);
    expect(host.calls.where((call) => call.startsWith('POST')).length, 1);
  });

  test('a refusal for lack of APPROVE locks the decisions until the screen shows again', () async {
    final host = ScriptedTransport({
      'GET /v1/plans': _list([_plan('plan_a', 'proposed')]),
      'POST /v1/plans/plan_a/approve': '{}',
    });
    final plans = _controller(host);
    addTearDown(plans.dispose);
    await plans.refresh();
    host.failNext(
      'POST /v1/plans/plan_a/approve',
      const HostError(403, '{}'),
    );

    final done = await plans.decide(plans.plans.single, PlanDecision.approve);

    expect(done, isFalse);
    expect(plans.denied, isTrue);
    expect(plans.failureOf('plan_a'), isNull);

    plans.setActive(true);
    expect(plans.denied, isFalse);
    plans.setActive(false);
  });

  test('a decision refused as decided keeps its failure and reads the plan again', () async {
    final host = ScriptedTransport({
      'GET /v1/plans': _list([_plan('plan_a', 'proposed')]),
    });
    final plans = _controller(host);
    addTearDown(plans.dispose);
    await plans.refresh();
    final decided = const HostError(409, '{"detail":{"code":"plan_decided"}}');
    host.failNext('POST /v1/plans/plan_a/approve', decided);

    final done = await plans.decide(plans.plans.single, PlanDecision.approve);

    expect(done, isFalse);
    expect(plans.failureOf('plan_a'), same(decided));
    expect(host.calls.where((call) => call == 'GET /v1/plans').length, 2);
  });
}
