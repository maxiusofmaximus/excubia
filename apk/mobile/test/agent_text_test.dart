import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/src/rust/api/state.dart';
import 'package:excubia/ui/widgets/agent_text.dart';

import 'support/agent_fixtures.dart';

void main() {
  group('agentEndedText', () {
    final now = DateTime(2026, 11, 20, 12);
    int ago(Duration d) => now.subtract(d).millisecondsSinceEpoch;

    test('a finished agent with a last movement says how long ago', () {
      final text = agentEndedText(
        agent(
          status: AgentStatus.idle,
          restored: true,
          lastActiveMs: ago(const Duration(hours: 2)),
        ),
        now: now,
      );
      expect(text, 'Terminó hace 2 h');
    });

    test('a finished agent just now reads «Terminó ahora mismo»', () {
      final text = agentEndedText(
        agent(
          status: AgentStatus.idle,
          restored: true,
          lastActiveMs: ago(Duration.zero),
        ),
        now: now,
      );
      expect(text, 'Terminó ahora mismo');
    });

    test('a finished agent without a timestamp shows nothing', () {
      expect(
        agentEndedText(
          agent(status: AgentStatus.idle, restored: true),
          now: now,
        ),
        isNull,
      );
    });

    test('a live agent with a timestamp shows nothing', () {
      expect(
        agentEndedText(
          agent(
            status: AgentStatus.working,
            lastActiveMs: ago(const Duration(hours: 2)),
          ),
          now: now,
        ),
        isNull,
      );
    });
  });

  group('stripRoleSuffix', () {
    test('takes the host role tag off the end', () {
      expect(stripRoleSuffix('Ernesto — lead'), 'Ernesto');
      expect(stripRoleSuffix('Ernesto - Lead'), 'Ernesto');
      expect(stripRoleSuffix('Ana — especialista'), 'Ana');
      expect(stripRoleSuffix('Ana - Specialist'), 'Ana');
    });

    test('leaves text without a role tag intact', () {
      expect(stripRoleSuffix('Leadership'), 'Leadership');
      expect(stripRoleSuffix('Plan de tres hitos'), 'Plan de tres hitos');
      expect(stripRoleSuffix('bench-proj'), 'bench-proj');
      expect(stripRoleSuffix('Ernesto lead'), 'Ernesto lead');
    });
  });

  group('agentSubtitle', () {
    test('an unknown state with no task shows nothing, not the state word', () {
      expect(
        agentSubtitle(AgentStatus.unknown, 'Sin actividad reciente', null),
        isNull,
      );
      expect(
        agentSubtitle(AgentStatus.unknown, 'Sin actividad reciente', '  '),
        isNull,
      );
    });

    test('an unknown state with a task shows the task alone', () {
      expect(
        agentSubtitle(
          AgentStatus.unknown,
          'Sin actividad reciente',
          'Plan de tres hitos — lead',
        ),
        'Plan de tres hitos',
      );
    });

    test('a pane title that is only a character name is not a task', () {
      expect(
        agentSubtitle(
          AgentStatus.unknown,
          'Sin actividad reciente',
          'Ernesto — lead',
        ),
        isNull,
      );
      expect(
        agentSubtitle(AgentStatus.working, 'trabajando', 'Ernesto — lead'),
        'trabajando',
      );
    });

    test('a known state shows its word, and the task after it', () {
      expect(
        agentSubtitle(AgentStatus.working, 'trabajando', null),
        'trabajando',
      );
      expect(
        agentSubtitle(AgentStatus.working, 'trabajando', 'Plan de tres hitos'),
        'trabajando · Plan de tres hitos',
      );
    });
  });
}
