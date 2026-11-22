import 'dart:convert';

import 'alerts_models.dart';
import 'chat_transport.dart';
import 'json_read.dart';

/// The alert rules and the alerts mode (`/v1/alerts`; contract `api.md`, «Reglas de aviso»).
/// Plain JSON over the signed transport. Reads need `READ_STATE`, writes `RESPOND`.
class AlertsApi {
  const AlertsApi(this._transport);

  final ChatTransport _transport;

  Future<List<AlertRule>> alertsRules() async {
    final body = await _read('/v1/alerts/rules');
    return [
      for (final item in jsonList(body, 'rules'))
        AlertRule.fromJson(jsonObject(item)),
    ];
  }

  /// [agent] and [status] are left out when they do not apply: `null` agent is any agent,
  /// and a reminder has neither.
  Future<AlertRule> createAlertRule({
    required AlertTrigger trigger,
    String? agent,
    AlertRuleStatus? status,
    required AlertAction action,
  }) async {
    final json = await _write('/v1/alerts/rules', {
      'trigger': trigger.wire,
      'agent': ?agent,
      'status': ?status?.wire,
      'action': action.wire,
    });
    return AlertRule.fromJson(jsonObject(json['rule']));
  }

  Future<AlertRule> setAlertRuleEnabled(String id, bool enabled) async {
    final json = await _write(_idPath(id, 'update'), {'enabled': enabled});
    return AlertRule.fromJson(jsonObject(json['rule']));
  }

  Future<void> deleteAlertRule(String id) =>
      _write(_idPath(id, 'delete'), const {});

  Future<AlertsMode> alertsMode() async =>
      wiredEnum(AlertsMode.values, (await _read('/v1/alerts/mode'))['mode']);

  Future<AlertsMode> setAlertsMode(AlertsMode mode) async => wiredEnum(
    AlertsMode.values,
    (await _write('/v1/alerts/mode', {'mode': mode.wire}))['mode'],
  );

  /// The agents on the PC by name, for the agent a rule can name. The state lists them
  /// as `agents[].agent`, once per agent, in alphabetical order.
  Future<List<String>> agentNames() async {
    final body = jsonObject(
      jsonDecode(await _transport.call('GET', '/v1/state')),
    );
    final names = <String>{
      for (final item in jsonList(body, 'agents'))
        if (jsonObject(item)['agent'] case final String name) name,
    };
    return names.toList()..sort();
  }

  Future<Map<String, Object?>> _read(String target) async =>
      jsonEnvelope(await _transport.call('GET', target));

  Future<Map<String, Object?>> _write(
    String target,
    Map<String, Object?> json,
  ) async => jsonEnvelope(
    await _transport.call('POST', target, utf8.encode(jsonEncode(json))),
  );

  static String _idPath(String id, String action) =>
      '/v1/alerts/rules/${Uri.encodeComponent(id)}/$action';
}
