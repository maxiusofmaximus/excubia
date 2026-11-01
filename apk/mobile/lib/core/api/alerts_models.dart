import 'json_read.dart';

/// A value the host names with a word of its own (`pane_status`, `rules_only`, ...).
abstract interface class Wired {
  String get wire;
}

enum AlertTrigger implements Wired {
  paneStatus('pane_status'),
  calendarReminder('calendar_reminder');

  const AlertTrigger(this.wire);

  @override
  final String wire;
}

/// The change of an agent's pane that a rule is about.
enum AlertRuleStatus implements Wired {
  blocked('blocked'),
  done('done');

  const AlertRuleStatus(this.wire);

  @override
  final String wire;
}

enum AlertAction implements Wired {
  notify('notify'),
  mute('mute');

  const AlertAction(this.wire);

  @override
  final String wire;
}

/// `all` alerts as it always did, `rules_only` only for the rules that say notify.
enum AlertsMode implements Wired {
  all('all'),
  rulesOnly('rules_only');

  const AlertsMode(this.wire);

  @override
  final String wire;
}

/// The value a host word names. An unknown word is a broken answer, not a choice.
T wiredEnum<T extends Wired>(Iterable<T> values, Object? wire) {
  for (final value in values) {
    if (value.wire == wire) return value;
  }
  throw FormatException('valor desconocido: $wire');
}

class AlertRule {
  const AlertRule({
    required this.id,
    required this.trigger,
    required this.agent,
    required this.status,
    required this.action,
    required this.enabled,
  });

  factory AlertRule.fromJson(Map<String, Object?> json) => AlertRule(
    id: jsonText(json, 'id'),
    trigger: wiredEnum(AlertTrigger.values, json['trigger']),
    agent: switch (json['agent']) {
      final String name => name,
      _ => null,
    },
    status: json['status'] == null
        ? null
        : wiredEnum(AlertRuleStatus.values, json['status']),
    action: wiredEnum(AlertAction.values, json['action']),
    enabled: json['enabled'] == true,
  );

  final String id;
  final AlertTrigger trigger;

  /// `null` means any agent.
  final String? agent;

  /// Only a rule on an agent has one.
  final AlertRuleStatus? status;
  final AlertAction action;
  final bool enabled;
}

/// The rule in words, as the list and the form show it.
String alertSentence({
  required AlertTrigger trigger,
  String? agent,
  AlertRuleStatus? status,
  required AlertAction action,
}) => switch (trigger) {
  AlertTrigger.calendarReminder =>
    action == AlertAction.notify
        ? 'Avisar de recordatorios del calendario'
        : 'Silenciar los recordatorios del calendario',
  AlertTrigger.paneStatus =>
    '${action == AlertAction.notify ? 'Avisar' : 'Silenciar'} cuando '
        '${agent ?? 'cualquier agente'} '
        '${status == AlertRuleStatus.done ? 'termine' : 'se bloquee'}',
};

extension AlertRuleSentence on AlertRule {
  String get sentence => alertSentence(
    trigger: trigger,
    agent: agent,
    status: status,
    action: action,
  );
}
