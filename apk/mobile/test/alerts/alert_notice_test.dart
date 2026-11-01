import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/alerts/alert_notice.dart';

String _state({int userApprovals = 0, int blocked = 0}) => jsonEncode({
  'counts': {'blocked': blocked, 'working': 1, 'idle': 0},
  'chat': {'pending_user_approvals': userApprovals},
  'agents': [
    {'pane_id': 'w1:p1', 'text': 'secret command'},
  ],
});

void main() {
  test('approvals waiting for the user come first, singular', () {
    final notice = AlertNotice.fromState(_state(userApprovals: 1, blocked: 3));
    expect(notice.body, 'Una aprobación espera tu decisión.');
  });

  test('approvals waiting for the user, plural', () {
    final notice = AlertNotice.fromState(_state(userApprovals: 4));
    expect(notice.body, '4 aprobaciones esperan tu decisión.');
  });

  test('blocked agents when no approval waits', () {
    final notice = AlertNotice.fromState(_state(blocked: 2));
    expect(notice.body, '2 agentes están bloqueados.');
  });

  test('nothing actionable falls back to the generic text', () {
    final notice = AlertNotice.fromState(_state());
    expect(notice.title, 'Excubia');
    expect(notice.body, 'Hay novedades de tus agentes.');
  });

  test('a state that does not parse falls back to the generic text', () {
    expect(AlertNotice.fromState('not json').body, AlertNotice.generic.body);
    expect(AlertNotice.fromState('[1,2]').body, AlertNotice.generic.body);
  });

  test('the alert text never carries agent text or pane ids', () {
    final notice = AlertNotice.fromState(_state(blocked: 1));
    expect(notice.body, isNot(contains('secret')));
    expect(notice.body, isNot(contains('w1:p1')));
  });
}
