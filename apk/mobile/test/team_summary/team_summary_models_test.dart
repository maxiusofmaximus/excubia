import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/team_summary_models.dart';

Map<String, Object?> member({
  String id = 'hire_1',
  String name = 'Bob',
  String status = 'idle',
  String? since = '2026-11-05T10:00:00.000Z',
}) => {
  'agent_id': id,
  'name': name,
  'role_title': 'Programador',
  'status': status,
  'headline': 'Sin actividad',
  'since': ?since,
};

Map<String, Object?> envelope(List<Map<String, Object?>> members) => {
  'ok': true,
  'members': members,
};

void main() {
  group('teamStatusOf', () {
    test('reads the five states the host names', () {
      expect(teamStatusOf('waiting_for_you'), TeamStatus.waitingForYou);
      expect(teamStatusOf('blocked'), TeamStatus.blocked);
      expect(teamStatusOf('working'), TeamStatus.working);
      expect(teamStatusOf('done'), TeamStatus.done);
      expect(teamStatusOf('idle'), TeamStatus.idle);
    });

    test('a state the host does not name reads as idle', () {
      expect(teamStatusOf('sleeping'), TeamStatus.idle);
      expect(teamStatusOf(''), TeamStatus.idle);
    });
  });

  group('teamMembersFrom', () {
    test('keeps the order the host sent, with no sorting of its own', () {
      final members = teamMembersFrom(
        envelope([
          member(id: 'a', name: 'Ana', status: 'waiting_for_you'),
          member(id: 'b', name: 'Bob', status: 'blocked'),
          member(id: 'c', name: 'Cy', status: 'idle'),
        ]),
      );

      expect(members.map((m) => m.agentId), ['a', 'b', 'c']);
      expect(members.first.status, TeamStatus.waitingForYou);
      expect(members.first.roleTitle, 'Programador');
      expect(members.first.headline, 'Sin actividad');
    });

    test('an empty list is an empty team, not an error', () {
      expect(teamMembersFrom(envelope([])), isEmpty);
    });

    test('a missing time leaves the time unknown, not the card', () {
      final members = teamMembersFrom(envelope([member(since: null)]));

      expect(members.single.since, isNull);
      expect(members.single.name, 'Bob');
    });

    test('an unreadable time is unknown too', () {
      final members = teamMembersFrom(envelope([member(since: 'ayer')]));

      expect(members.single.since, isNull);
    });

    test('a member without a name is a broken answer', () {
      final broken = member()..remove('name');

      expect(() => teamMembersFrom(envelope([broken])), throwsFormatException);
    });

    test('a list that is not a list is a broken answer', () {
      expect(
        () => teamMembersFrom({'ok': true, 'members': 'Bob'}),
        throwsFormatException,
      );
    });
  });
}
