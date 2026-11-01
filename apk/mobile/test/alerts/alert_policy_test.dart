import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/alerts/alert_policy.dart';
import 'package:herdr_notify/core/alerts/manufacturer_guide.dart';

void main() {
  group('alertBackoff', () {
    test('steps through the table and then repeats its last step', () {
      final seconds = [
        for (var attempt = 1; attempt <= 10; attempt++)
          alertBackoff(attempt).inSeconds,
      ];
      expect(seconds, [5, 10, 15, 20, 30, 45, 60, 120, 120, 120]);
    });

    test('a zero or negative attempt waits the first step', () {
      expect(alertBackoff(0).inSeconds, 5);
      expect(alertBackoff(-3).inSeconds, 5);
    });
  });

  group('sseIdOf', () {
    test('takes the value after the colon, dropping one leading space', () {
      expect(sseIdOf('id: 7'), '7');
      expect(sseIdOf('id:7'), '7');
      expect(sseIdOf('id:  7'), ' 7');
    });

    test('an empty id is kept as empty, so the caller can clear it', () {
      expect(sseIdOf('id:'), '');
    });

    test('other lines carry no id', () {
      expect(sseIdOf(': comentario'), isNull);
      expect(sseIdOf('data: id: 3'), isNull);
      expect(sseIdOf(''), isNull);
    });
  });

  group('alertKindOf', () {
    test('an approval alerts as approval', () {
      expect(
        alertKindOf(const AlertFrame(event: 'chat.approval')),
        AlertKind.approval,
      );
    });

    test('only a finished delegation alerts as review', () {
      expect(
        alertKindOf(const AlertFrame(event: 'chat.delegation', status: 'done')),
        AlertKind.review,
      );
      expect(
        alertKindOf(
          const AlertFrame(event: 'chat.delegation', status: 'running'),
        ),
        isNull,
      );
      expect(alertKindOf(const AlertFrame(event: 'chat.message')), isNull);
    });

    test('an alert frame alerts as the kind the host decided', () {
      expect(
        alertKindOf(const AlertFrame(event: 'alert', alert: AlertKind.blocked)),
        AlertKind.blocked,
      );
      expect(
        alertKindOf(const AlertFrame(event: 'alert', alert: AlertKind.done)),
        AlertKind.done,
      );
      expect(
        alertKindOf(
          const AlertFrame(event: 'alert', alert: AlertKind.reminder),
        ),
        AlertKind.reminder,
      );
    });

    test('an alert frame with no known kind is ignored', () {
      expect(alertKindOf(const AlertFrame(event: 'alert')), isNull);
    });

    test('when the host decides, a finished task no longer alerts by itself', () {
      expect(
        alertKindOf(
          const AlertFrame(event: 'chat.delegation', status: 'done'),
          hostDecides: true,
        ),
        isNull,
      );
    });

    test('when the host decides, approvals still alert', () {
      expect(
        alertKindOf(
          const AlertFrame(event: 'chat.approval'),
          hostDecides: true,
        ),
        AlertKind.approval,
      );
    });

    test('each kind uses its channel', () {
      expect(AlertKind.approval.channel, 'agents');
      expect(AlertKind.review.channel, 'agents-review');
      expect(AlertKind.blocked.channel, 'agents');
      expect(AlertKind.done.channel, 'agents-review');
      expect(AlertKind.reminder.channel, 'recordatorios');
    });
  });

  group('hostAlertKindOf', () {
    test('reads the kind of an alert frame and ignores the rest of it', () {
      expect(
        hostAlertKindOf('data: {"kind":"agent_blocked","rule_id":"ar_1","ts":"t"}'),
        AlertKind.blocked,
      );
      expect(
        hostAlertKindOf('data: {"kind":"agent_done","rule_id":null,"ts":"t"}'),
        AlertKind.done,
      );
      expect(
        hostAlertKindOf('data: {"kind":"calendar_reminder","rule_id":"ar_2","ts":"t"}'),
        AlertKind.reminder,
      );
    });

    test('an unknown kind, a body that is not an object or a missing data line is ignored', () {
      expect(hostAlertKindOf('data: {"kind":"otra_cosa"}'), isNull);
      expect(hostAlertKindOf('data: no es json'), isNull);
      expect(hostAlertKindOf('data: [1]'), isNull);
      expect(hostAlertKindOf(''), isNull);
    });
  });

  group('hostDecidesAlerts', () {
    test('a state with alert_rules means the host decides which changes alert', () {
      expect(
        hostDecidesAlerts('{"alert_rules":{"mode":"all","enabled_count":0}}'),
        isTrue,
      );
    });

    test('an older host, or a state that is not JSON, keeps the old alerts', () {
      expect(hostDecidesAlerts('{"chat":{}}'), isFalse);
      expect(hostDecidesAlerts('no es json'), isFalse);
    });
  });

  group('manufacturerSteps', () {
    test('TECNO, Infinix and itel get the HiOS steps', () {
      for (final maker in ['tecno', 'infinix', 'itel']) {
        expect(manufacturerSteps(maker), hiosSteps, reason: maker);
      }
      expect(hiosSteps, hasLength(3));
    });

    test('other makers get no guide here', () {
      expect(manufacturerSteps('samsung'), isNull);
      expect(manufacturerSteps('xiaomi'), isNull);
    });
  });
}
