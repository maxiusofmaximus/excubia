import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/chat_transport.dart';
import 'package:herdr_notify/core/api/connection_failure.dart';
import 'package:herdr_notify/core/api/connection_text.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/api/restore_api.dart';

/// Answers by exact route ("METHOD target"), the query included. A [HostError]
/// answer is thrown the way the real client refuses.
class _Host implements ChatTransport {
  _Host(this.answers);

  final Map<String, Object> answers;
  final List<({String method, String target, String body})> calls = [];

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    calls.add((method: method, target: target, body: utf8.decode(body)));
    final answer = answers['$method $target'];
    if (answer is HostError) throw answer;
    if (answer is String) return answer;
    throw const HostError(404, '{"detail":"not found"}');
  }

  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError();

  @override
  Future<List<int>> callBytes(String method, String target) =>
      throw UnimplementedError();
}

String _ok(Map<String, Object?> fields) => jsonEncode({'ok': true, ...fields});

Map<String, Object?> _point(String id, int createdMs) => {
  'id': id,
  'kind': 'turn',
  'created_ms': createdMs,
  'files': 1,
  'excluded': 0,
};

HostError _coded(int status, String code) => HostError(
  status,
  jsonEncode({
    'detail': {'code': code, 'message': 'x'},
  }),
);

void main() {
  group('RestoreApi routes', () {
    test('lists the copies of an agent with a GET, newest first', () async {
      final host = _Host({
        'GET /v1/restore/points?agent=w1%3Ap1': _ok({
          'agent': 'w1:p1',
          'points': [_point('01A', 1), _point('01B', 7)],
        }),
      });

      final points = await RestoreApi(host).points('w1:p1');

      expect(host.calls.single.method, 'GET');
      expect(host.calls.single.target, '/v1/restore/points?agent=w1%3Ap1');
      expect(points.map((point) => point.id), ['01B', '01A']);
    });

    test('reads the changes of one copy with a GET', () async {
      final host = _Host({
        'GET /v1/restore/points/01A/files': _ok({
          'point': '01A',
          'changes': [
            {'path': 'a.md', 'status': 'modified'},
          ],
          'excluded': <Object>[],
        }),
      });

      final changes = await RestoreApi(host).changes('01A');

      expect(host.calls.single.target, '/v1/restore/points/01A/files');
      expect(changes.changes.single.path, 'a.md');
    });

    test('restores a copy with a signed POST, the same as an approval', () async {
      final host = _Host({
        'POST /v1/restore/points/01A/restore': _ok({
          'point': '01A',
          'restored': 1,
          'removed': 0,
          'undo_point': '01U',
        }),
      });

      final result = await RestoreApi(host).restore('01A');

      expect(host.calls.single.method, 'POST');
      expect(host.calls.single.target, '/v1/restore/points/01A/restore');
      expect(host.calls.single.body, '{}');
      expect(result.undoPoint, '01U');
    });
  });

  group('restoreFailureText', () {
    test('words each code the host sends for a restore', () {
      expect(
        restoreFailureText(_coded(404, 'restore_point_not_found')),
        'Esa copia ya no existe. Vuelve a la lista.',
      );
      expect(
        restoreFailureText(_coded(400, 'restore_invalid')),
        'La copia tiene rutas que salen de la carpeta, así que no se ha restaurado nada.',
      );
      expect(
        restoreFailureText(_coded(409, 'restore_failed')),
        'No se pudo volver a esa copia. Revisa la carpeta y reinténtalo.',
      );
    });

    test('says the same as the desktop when the grant is missing', () {
      expect(
        restoreFailureText(const HostError(403, '{"detail":"forbidden"}')),
        restoreGrantLede,
      );
    });

    test('says the PC is unreachable in plain words, not the host code', () {
      final error = const HostError.transport(
        ConnectionFailure.noRoute,
        'socket',
      );

      expect(restoreFailureText(error), connectionText(ConnectionFailure.noRoute));
    });

    test('falls back to one plain line for an answer it does not know', () {
      expect(
        restoreFailureText(const HostError(500, 'boom')),
        'El equipo no pudo completar la acción. Inténtalo de nuevo.',
      );
    });

    test('reads a bare 404 without a code as a missing copy', () {
      expect(
        restoreFailureText(const HostError(404, '{"detail":"not found"}')),
        'Ya no existe. Vuelve a la lista.',
      );
    });

    test('keeps a non-host error as the plain line', () {
      expect(
        restoreFailureText(StateError('x')),
        'El equipo no pudo completar la acción. Inténtalo de nuevo.',
      );
    });
  });

  group('restoreWaitingText', () {
    test('says the device waits for its grant on a 403 to a read', () {
      expect(
        restoreWaitingText(const HostError(403, '{"detail":"forbidden"}')),
        'Esperando que aprueben este dispositivo.',
      );
    });

    test('is null for any other failure, so the error is shown', () {
      expect(restoreWaitingText(_coded(404, 'restore_point_not_found')), isNull);
    });
  });
}
