import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/src/rust/api/chat/model_team.dart';
import 'package:excubia/ui/chat/approval_format.dart';

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
    expect(levelLabel('lead'), 'lo decide el responsable del proyecto');
    expect(levelLabel('policy'), isNull);
  });

  test('the title names the lead seat, or falls back to "Un agente"', () {
    expect(agentTitle('seat_bob', _projects, null), 'Bob');
    expect(agentTitle('seat_ghost', _projects, null), 'Un agente');
    expect(agentTitle(null, _projects, null), 'Un agente');
    expect(projectNameFor('seat_bob', _projects), 'Demo');
    expect(projectNameFor('seat_ghost', _projects), isNull);
  });

  test('the title names Alfred by his seat, never by a project', () {
    const alfred = ChatSeat(
      agentId: 'seat_alfred',
      name: 'Alfred',
      status: 'working',
    );
    expect(agentTitle('seat_alfred', _projects, alfred), 'Alfred');
    expect(agentTitle('seat_bob', _projects, alfred), 'Bob');
    expect(agentTitle('seat_ghost', _projects, alfred), 'Un agente');
    expect(agentTitle('seat_alfred', _projects, null), 'Un agente');
    expect(agentTitle(null, _projects, alfred, tool: 'mail.move'), 'Alfred');
    expect(agentTitle(null, _projects, alfred, tool: 'bash'), 'Un agente');
    expect(projectNameFor('seat_alfred', _projects), isNull);
  });

  test('a mail destination and its triage verdict read as the card says', () {
    expect(destLabel('junkemail'), 'No deseado');
    expect(destLabel('deleteditems'), 'Eliminados');
    expect(destLabel('inbox'), 'Bandeja de entrada');
    expect(destLabel('archive'), 'Destino desconocido');
    expect(triageLabel('phish_candidate'), 'posible phishing');
    expect(triageLabel('clean'), 'sin señales automáticas');
    expect(triageLabel('weird'), isNull);
    expect(triageLabel(null), isNull);
  });
}
