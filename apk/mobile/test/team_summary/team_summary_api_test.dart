import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/chat_transport.dart';
import 'package:excubia/core/api/connection_failure.dart';
import 'package:excubia/core/api/connection_text.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/team_summary_api.dart';
import 'package:excubia/core/api/team_summary_models.dart';

/// Answers the one route the summary reads.
class _Host implements ChatTransport {
  _Host({this.answer});

  final String? answer;
  final List<String> routes = [];

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    routes.add('$method $target');
    return answer!;
  }

  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError();

  @override
  Future<List<int>> callBytes(String method, String target) =>
      throw UnimplementedError();
}

String _members() => jsonEncode({
  'ok': true,
  'members': [
    {
      'agent_id': 'hire_1',
      'name': 'Bob',
      'role_title': 'Programador',
      'status': 'waiting_for_you',
      'headline': 'Espera tu visto bueno',
      'since': '2026-11-05T10:00:00.000Z',
    },
  ],
});

void main() {
  test('reads the summary with one GET and returns its members', () async {
    final host = _Host(answer: _members());

    final members = await TeamSummaryApi(host).members();

    expect(host.routes, ['GET /v1/team/summary']);
    expect(members.single.name, 'Bob');
    expect(members.single.status, TeamStatus.waitingForYou);
  });

  test('an answer that is not ok is broken, not an empty team', () async {
    final host = _Host(answer: '{"ok": false, "members": []}');

    expect(TeamSummaryApi(host).members(), throwsFormatException);
  });

  group('teamSummaryWaitingText', () {
    test('a 403 says the device still waits for its grant', () {
      expect(
        teamSummaryWaitingText(const HostError(403, '{"detail":"forbidden"}')),
        'Esperando que aprueben este dispositivo.',
      );
    });

    test('any other failure is not a wait', () {
      expect(teamSummaryWaitingText(const HostError(500, '{}')), isNull);
    });
  });

  group('teamSummaryFailureText', () {
    test('a link failure keeps its own words', () {
      expect(
        teamSummaryFailureText(
          const HostError.transport(ConnectionFailure.hostDown, ''),
        ),
        connectionText(ConnectionFailure.hostDown),
      );
    });

    test('a 403 is the wait, not an error', () {
      expect(
        teamSummaryFailureText(const HostError(403, '{}')),
        'Esperando que aprueben este dispositivo.',
      );
    });

    test('any other answer says it could not be read, and can be retried', () {
      expect(
        teamSummaryFailureText(const HostError(500, '{}')),
        'No se pudo leer tu equipo. Inténtalo de nuevo.',
      );
      expect(
        teamSummaryFailureText(StateError('x')),
        'No se pudo leer tu equipo. Inténtalo de nuevo.',
      );
    });
  });
}
