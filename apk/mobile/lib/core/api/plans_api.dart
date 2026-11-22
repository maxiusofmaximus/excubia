import 'dart:convert';

import 'chat_transport.dart';
import 'connection_text.dart';
import 'host_client.dart';
import 'json_read.dart';
import 'plan_models.dart';
import 'read_grant.dart';

part 'plans_failure_text.dart';

/// The plan routes, `/v1/plans` (contract `api.md`, F3). Reads need `READ_STATE`; a decision
/// needs `APPROVE`, and goes through the same signed transport an approval does.
class PlansApi {
  const PlansApi(this._transport);

  final ChatTransport _transport;

  Future<List<TeamPlan>> plans() async => teamPlansFrom(
    jsonEnvelope(await _transport.call('GET', '/v1/plans')),
  );

  /// Answers one plan. Asking for changes needs its note; the other two send an empty body.
  Future<TeamPlan> answer(
    String planId,
    PlanDecision decision, {
    String? note,
  }) async {
    final body = switch ((decision, note)) {
      (PlanDecision.requestChanges, final String text) => utf8.encode(
        jsonEncode({'note': text}),
      ),
      (PlanDecision.requestChanges, _) => throw ArgumentError.value(
        note,
        'note',
        'pedir cambios exige una nota',
      ),
      _ => utf8.encode('{}'),
    };
    final path =
        '/v1/plans/${Uri.encodeComponent(planId)}/${_segment(decision)}';
    return teamPlanFrom(
      jsonEnvelope(await _transport.call('POST', path, body)),
    );
  }

  static String _segment(PlanDecision decision) => switch (decision) {
    PlanDecision.approve => 'approve',
    PlanDecision.reject => 'reject',
    PlanDecision.requestChanges => 'request_changes',
  };
}
