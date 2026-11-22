import 'dart:convert';

import 'chat_transport.dart';
import 'connection_text.dart';
import 'host_client.dart';
import 'json_read.dart';
import 'read_grant.dart';
import 'restore_models.dart';

part 'restore_failure_text.dart';

/// The restore routes, `/v1/restore` (contract `api.md`, F2). Reads need `READ_STATE`;
/// the restore and its undo need `APPROVE`, signed the way an approval is signed.
class RestoreApi {
  const RestoreApi(this._transport);

  final ChatTransport _transport;

  /// The copies of one agent, newest first. [agent] is the pane id, which the host
  /// does not keep across a restart: the caller reads it again rather than store it.
  Future<List<RestorePoint>> points(String agent) async {
    final body = jsonEnvelope(
      await _transport.call(
        'GET',
        Uri(
          path: '/v1/restore/points',
          queryParameters: {'agent': agent},
        ).toString(),
      ),
    );
    return sortRestorePoints([
      for (final item in jsonList(body, 'points'))
        RestorePoint.fromJson(jsonObject(item)),
    ]);
  }

  Future<RestoreChanges> changes(String pointId) async =>
      RestoreChanges.fromJson(
        jsonEnvelope(
          await _transport.call('GET', _pointPath(pointId, 'files')),
        ),
      );

  /// Puts the folder back as the copy has it. The undo is this same call on the copy it made.
  Future<RestoreResult> restore(String pointId) async => RestoreResult.fromJson(
    jsonEnvelope(
      await _transport.call(
        'POST',
        _pointPath(pointId, 'restore'),
        utf8.encode('{}'),
      ),
    ),
  );

  static String _pointPath(String pointId, String action) =>
      '/v1/restore/points/${Uri.encodeComponent(pointId)}/$action';
}
