import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/schedule_models.dart';
import 'package:excubia/core/api/schedules_api.dart';

import '../support/scripted_transport.dart';

const _scheduleJson =
    '{"id":"sch_1","title":"Resumen","conversation_id":"conv_1",'
    '"role_id":"audio-editor","body":"texto","recipe_id":null,"rule":null,'
    '"start_date":"2026-11-12","time_local":"09:00","timezone":"Europe/Madrid",'
    '"steps":1,"status":"paused","plan_id":null,"next_run_at":null,'
    '"last_run_at":null,"created_at":"2026-11-01T10:00:00.000Z",'
    '"updated_at":"2026-11-01T10:00:00.000Z"}';

String _one(String schedule) => '{"ok":true,"schedule":$schedule}';

void main() {
  group('reading the schedules', () {
    test('one GET of the list, parsed in the host order', () async {
      final host = ScriptedTransport({
        'GET /v1/schedules': '{"ok":true,"schedules":[$_scheduleJson]}',
      });

      final schedules = await SchedulesApi(host).schedules();

      expect(host.calls, ['GET /v1/schedules']);
      expect(schedules.single.id, 'sch_1');
      expect(schedules.single.status, ScheduleStatus.paused);
    });

    test('the runs of one schedule come from its own route', () async {
      final host = ScriptedTransport({
        'GET /v1/schedules/sch_1/runs': '{"ok":true,"runs":[]}',
      });

      final runs = await SchedulesApi(host).runs('sch_1');

      expect(host.calls, ['GET /v1/schedules/sch_1/runs']);
      expect(runs, isEmpty);
    });
  });

  group('changing a schedule', () {
    test(
      'pause and resume send an empty JSON object to their own route',
      () async {
        final host = ScriptedTransport({
          'POST /v1/schedules/sch_1/pause': _one(_scheduleJson),
          'POST /v1/schedules/sch_1/resume': _one(_scheduleJson),
        });

        await SchedulesApi(host).pause('sch_1');
        final resumed = await SchedulesApi(host).resume('sch_1');

        expect(resumed.id, 'sch_1');
        expect(host.calls, [
          'POST /v1/schedules/sch_1/pause',
          'POST /v1/schedules/sch_1/resume',
        ]);
        expect(host.bodies['POST /v1/schedules/sch_1/pause'], [{}]);
      },
    );

    test('the id is encoded so it cannot leave its route', () async {
      final host = ScriptedTransport({
        'POST /v1/schedules/a%2Fb/pause': _one(_scheduleJson),
      });

      await SchedulesApi(host).pause('a/b');

      expect(host.calls, ['POST /v1/schedules/a%2Fb/pause']);
    });

    test('«Lanzar ahora» answers with the launch it made', () async {
      final host = ScriptedTransport({
        'POST /v1/schedules/sch_1/run_now':
            '{"ok":true,"delegation_id":"del_1","status":"queued","depth":1}',
      });

      final launch = await SchedulesApi(host).runNow('sch_1');

      expect(launch.launched, isTrue);
      expect(host.calls, ['POST /v1/schedules/sch_1/run_now']);
    });
  });

  group('the words of a refused action', () {
    HostError coded(int status, String code) => HostError(
      status,
      jsonEncode({
        'detail': {'code': code},
      }),
    );

    test('a schedule that is gone says so and leaves the list', () {
      expect(
        scheduleActionFailureText(coded(404, 'no_such_schedule')),
        'Esa tarea ya no existe. Se quitará de la lista.',
      );
    });

    test('a finished schedule cannot be changed', () {
      expect(
        scheduleActionFailureText(coded(409, 'schedule_done')),
        'Esta tarea ya está hecha: no se puede cambiar.',
      );
    });

    test('a device without APPROVE is told how to get it', () {
      expect(
        scheduleActionFailureText(HostError(403, '')),
        schedulePermissionText,
      );
    });

    test('an unknown refusal says the action did not finish', () {
      expect(
        scheduleActionFailureText(coded(500, 'boom')),
        'No se pudo completar la acción. Inténtalo de nuevo.',
      );
    });
  });
}
