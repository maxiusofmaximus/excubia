import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/src/rust/api/state.dart';
import 'package:excubia/ui/widgets/agent_avatar.dart';
import 'package:excubia/ui/widgets/agent_text.dart';

import 'support/agent_fixtures.dart';

void main() {
  group('agentNameOf', () {
    test('a pane titled after a character names it, whatever the CLI id', () {
      expect(
        agentNameOf(
          agent(
            status: AgentStatus.idle,
            title: 'Ernesto — lead',
            cli: 'unknown',
          ),
        ),
        'Ernesto',
      );
      expect(
        agentNameOf(agent(status: AgentStatus.idle, title: 'Ernesto — lead')),
        'Ernesto',
      );
    });

    test('the seat label in the middle of the title still wins', () {
      expect(
        agentNameOf(
          agent(
            status: AgentStatus.idle,
            title: 'desconocido · Ana · especialista',
          ),
        ),
        'Ana',
      );
    });

    test('a title that is not a character falls back to the CLI id', () {
      expect(
        agentNameOf(
          agent(status: AgentStatus.idle, title: 'Plan de tres hitos — lead'),
        ),
        'Opencode',
      );
    });

    test('no title and no CLI id reads Especialista', () {
      expect(
        agentNameOf(
          agent(status: AgentStatus.idle, title: '', cli: 'desconocido'),
        ),
        'Especialista',
      );
      expect(
        agentNameOf(agent(status: AgentStatus.idle, cli: 'unknown')),
        'Especialista',
      );
    });
  });

  group('agentProjectLabel', () {
    test('the app name, in any case or empty, shows the character instead', () {
      for (final project in ['Excubia', 'excubia', 'core', 'Core', '  ', '']) {
        final entry = agent(
          status: AgentStatus.idle,
          title: 'Ernesto — lead',
          project: project,
        );
        expect(agentHasProject(entry), isFalse, reason: project);
        expect(agentProjectLabel(entry), 'Ernesto', reason: project);
      }
    });

    test('a real project shows as it is', () {
      final entry = agent(
        status: AgentStatus.idle,
        title: 'Ernesto — lead',
        project: 'Bench',
      );
      expect(agentHasProject(entry), isTrue);
      expect(agentProjectLabel(entry), 'Bench');
    });
  });
}
