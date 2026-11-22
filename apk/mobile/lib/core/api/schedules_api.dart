import 'dart:convert';

import 'chat_transport.dart';
import 'connection_text.dart';
import 'host_client.dart';
import 'json_read.dart';
import 'read_grant.dart';
import 'schedule_models.dart';

part 'schedules_failure_text.dart';

/// The schedule routes, `/v1/schedules` (contract `api.md`, F4). Reads need `READ_STATE`; pausing,
/// resuming and launching need `APPROVE`, signed like an approval. This device never creates,
/// edits or deletes a schedule: that is done from the desktop.
class SchedulesApi {
  const SchedulesApi(this._transport);

  final ChatTransport _transport;

  Future<List<Schedule>> schedules() async => schedulesFrom(
    jsonEnvelope(await _transport.call('GET', '/v1/schedules')),
  );

  /// The last runs of one schedule, newest first.
  Future<List<ScheduleRun>> runs(String scheduleId) async => runsFrom(
    jsonEnvelope(await _transport.call('GET', _path(scheduleId, 'runs'))),
  );

  Future<Schedule> pause(String scheduleId) => _change(scheduleId, 'pause');

  Future<Schedule> resume(String scheduleId) => _change(scheduleId, 'resume');

  /// Launches the task now. The rhythm does not move; a brake that stops it is answered as a run.
  Future<ScheduleLaunch> runNow(String scheduleId) async =>
      ScheduleLaunch.fromJson(
        jsonEnvelope(
          await _transport.call(
            'POST',
            _path(scheduleId, 'run_now'),
            utf8.encode('{}'),
          ),
        ),
      );

  Future<Schedule> _change(String scheduleId, String action) async {
    final answer = jsonEnvelope(
      await _transport.call(
        'POST',
        _path(scheduleId, action),
        utf8.encode('{}'),
      ),
    );
    return Schedule.fromJson(jsonObject(answer['schedule']));
  }

  static String _path(String scheduleId, String action) =>
      '/v1/schedules/${Uri.encodeComponent(scheduleId)}/$action';
}
