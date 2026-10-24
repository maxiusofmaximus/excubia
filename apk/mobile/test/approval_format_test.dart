import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';
import 'package:herdr_notify/ui/chat/approval_format.dart';

const _lead = ChatSeat(agentId: 'seat_bob', name: 'Bob', status: 'working');

const _projects = [
  ChatProject(
    id: 'prj_a',
    name: 'Demo',
    rootLabel: 'demo',
    status: 'idle',
    lead: _lead,
  ),
  ChatProject(id: 'prj_b', name: 'Otro', rootLabel: 'otro', status: 'idle'),
];

void main() {
  test('the countdown reads m:ss and never goes negative', () {
    expect(formatCountdown(125), '2:05');
    expect(formatCountdown(9), '0:09');
    expect(formatCountdown(-3), '0:00');
  });

  test('seconds come from the host deadline, not from a local TTL', () {
    final now = DateTime.utc(2026, 10, 23, 10);
    expect(secondsLeft('2026-10-23T10:02:05.000Z', now), 125);
    expect(secondsLeft(null, now), isNull);
    expect(secondsLeft('no es una fecha', now), isNull);
  });

  test('the working directory keeps its last two folders', () {
    expect(shortCwd(r'C:\Users\Max\demo\api'), '…/demo/api');
    expect(shortCwd('demo/api'), 'demo/api');
    expect(shortCwd('api'), 'api');
    expect(shortCwd('  '), isNull);
    expect(shortCwd(null), isNull);
  });

  test('risk and level chips read in Spanish', () {
    expect(riskLabel('high'), 'riesgo alto');
    expect(riskLabel('medium'), 'riesgo medio');
    expect(riskLabel(''), isNull);
    expect(riskLabel(null), isNull);
    expect(levelLabel('user'), 'te lo pasa Alfred');
    expect(levelLabel('cto'), 'lo decide Alfred');
    expect(levelLabel('lead'), 'lo decide el lead');
    expect(levelLabel('policy'), isNull);
  });

  test('the title names the lead seat, or falls back to "Un agente"', () {
    expect(agentTitle('seat_bob', _projects), 'Bob');
    expect(agentTitle('seat_ghost', _projects), 'Un agente');
    expect(agentTitle(null, _projects), 'Un agente');
    expect(projectNameFor('seat_bob', _projects), 'Demo');
    expect(projectNameFor('seat_ghost', _projects), isNull);
  });
}
