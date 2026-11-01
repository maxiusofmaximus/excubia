import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/alerts_models.dart';

void main() {
  group('frases de una regla', () {
    String say({
      AlertTrigger trigger = AlertTrigger.paneStatus,
      String? agent,
      AlertRuleStatus? status = AlertRuleStatus.done,
      AlertAction action = AlertAction.notify,
    }) => alertSentence(
      trigger: trigger,
      agent: agent,
      status: status,
      action: action,
    );

    test('un agente con nombre dice cuándo termina o se bloquea', () {
      expect(say(agent: 'Bob'), 'Avisar cuando Bob termine');
      expect(
        say(agent: 'Bob', status: AlertRuleStatus.blocked),
        'Avisar cuando Bob se bloquee',
      );
    });

    test('sin agente dice «cualquier agente», y silenciar se dice igual', () {
      expect(
        say(action: AlertAction.mute, status: AlertRuleStatus.blocked),
        'Silenciar cuando cualquier agente se bloquee',
      );
      expect(
        say(agent: 'Ana', action: AlertAction.mute),
        'Silenciar cuando Ana termine',
      );
    });

    test('un recordatorio del calendario no habla de agentes', () {
      expect(
        say(trigger: AlertTrigger.calendarReminder, status: null),
        'Avisar de recordatorios del calendario',
      );
      expect(
        say(
          trigger: AlertTrigger.calendarReminder,
          status: null,
          action: AlertAction.mute,
        ),
        'Silenciar los recordatorios del calendario',
      );
    });
  });

  group('AlertRule.fromJson', () {
    test('reads a rule on an agent', () {
      final rule = AlertRule.fromJson({
        'id': 'ar_1',
        'trigger': 'pane_status',
        'agent': 'Bob',
        'status': 'done',
        'action': 'notify',
        'enabled': true,
        'created_by': 'user',
        'created_at': '2026-10-29T09:00:00.000Z',
      });

      expect(rule.id, 'ar_1');
      expect(rule.trigger, AlertTrigger.paneStatus);
      expect(rule.agent, 'Bob');
      expect(rule.status, AlertRuleStatus.done);
      expect(rule.action, AlertAction.notify);
      expect(rule.enabled, isTrue);
      expect(rule.sentence, 'Avisar cuando Bob termine');
    });

    test('a calendar rule has no agent and no status', () {
      final rule = AlertRule.fromJson({
        'id': 'ar_2',
        'trigger': 'calendar_reminder',
        'agent': null,
        'status': null,
        'action': 'mute',
        'enabled': false,
      });

      expect(rule.trigger, AlertTrigger.calendarReminder);
      expect(rule.agent, isNull);
      expect(rule.status, isNull);
      expect(rule.enabled, isFalse);
    });

    test('a word the app does not know is a broken answer', () {
      expect(
        () => AlertRule.fromJson({
          'id': 'ar_3',
          'trigger': 'otra_cosa',
          'action': 'notify',
          'enabled': true,
        }),
        throwsFormatException,
      );
    });
  });
}
