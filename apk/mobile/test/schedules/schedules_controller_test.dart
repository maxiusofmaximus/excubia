import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/schedule_models.dart';
import 'package:excubia/core/api/schedules_api.dart';
import 'package:excubia/core/state/schedules_controller.dart';

import '../support/scripted_transport.dart';

const _active =
    '{"id":"sch_1","title":"Resumen","conversation_id":"conv_1",'
    '"role_id":"audio-editor","body":"texto","recipe_id":null,"rule":null,'
    '"start_date":"2026-11-12","time_local":"09:00","timezone":"Europe/Madrid",'
    '"steps":1,"status":"active","plan_id":null,'
    '"next_run_at":"2026-11-12T08:00:00.000Z","last_run_at":null,'
    '"created_at":"2026-11-01T10:00:00.000Z",'
    '"updated_at":"2026-11-01T10:00:00.000Z"}';

const _paused =
    '{"id":"sch_1","title":"Resumen","conversation_id":"conv_1",'
    '"role_id":"audio-editor","body":"texto","recipe_id":null,"rule":null,'
    '"start_date":"2026-11-12","time_local":"09:00","timezone":"Europe/Madrid",'
    '"steps":1,"status":"paused","plan_id":null,"next_run_at":null,'
    '"last_run_at":null,"created_at":"2026-11-01T10:00:00.000Z",'
    '"updated_at":"2026-11-01T10:00:00.000Z"}';

SchedulesController _controller(ScriptedTransport host) => SchedulesController(
  api: SchedulesApi(host),
  interval: const Duration(hours: 1),
);

void main() {
  group('reading', () {
    test(
      'the first answer marks the list as loaded, even when empty',
      () async {
        final host = ScriptedTransport({
          'GET /v1/schedules': '{"ok":true,"schedules":[]}',
        });
        final controller = _controller(host);
        addTearDown(controller.dispose);

        expect(controller.loaded, isFalse);
        await controller.refresh();

        expect(controller.loaded, isTrue);
        expect(controller.schedules, isEmpty);
      },
    );

    test('a refused read keeps its failure and no list', () async {
      final host = ScriptedTransport({});
      host.failNext('GET /v1/schedules', HostError(403, ''));
      final controller = _controller(host);
      addTearDown(controller.dispose);

      await controller.refresh();

      expect(controller.error, isA<HostError>());
      expect(controller.loaded, isFalse);
    });
  });

  group('pausing and resuming', () {
    test(
      'a pause puts the answered schedule in place of the old one',
      () async {
        final host = ScriptedTransport({
          'GET /v1/schedules': '{"ok":true,"schedules":[$_active]}',
          'POST /v1/schedules/sch_1/pause': '{"ok":true,"schedule":$_paused}',
        });
        final controller = _controller(host);
        addTearDown(controller.dispose);
        await controller.refresh();

        await controller.changeStatus(
          controller.schedules.single,
          ScheduleChange.pause,
        );

        expect(controller.failureOf('sch_1'), isNull);
        expect(controller.schedules.single.status, ScheduleStatus.paused);
      },
    );

    test('a refusal for lack of APPROVE locks the actions until the screen shows again', () async {
      final host = ScriptedTransport({
        'GET /v1/schedules': '{"ok":true,"schedules":[$_active]}',
      });
      host.failNext('POST /v1/schedules/sch_1/pause', HostError(403, ''));
      final controller = _controller(host);
      addTearDown(controller.dispose);
      await controller.refresh();
      final schedule = controller.schedules.single;

      await controller.changeStatus(schedule, ScheduleChange.pause);
      await controller.changeStatus(schedule, ScheduleChange.pause);

      expect(controller.denied, isTrue);
      expect(host.calls.where((call) => call.endsWith('/pause')), hasLength(1));
      controller.setActive(true);
      expect(controller.denied, isFalse);
    });

    test('a refused change keeps its words on the schedule only', () async {
      final host = ScriptedTransport({
        'GET /v1/schedules': '{"ok":true,"schedules":[$_active]}',
      });
      host.failNext(
        'POST /v1/schedules/sch_1/resume',
        HostError(409, '{"detail":{"code":"schedule_no_future_run"}}'),
      );
      final controller = _controller(host);
      addTearDown(controller.dispose);
      await controller.refresh();

      await controller.changeStatus(
        controller.schedules.single,
        ScheduleChange.resume,
      );

      expect(controller.failureOf('sch_1'), isA<HostError>());
      expect(controller.denied, isFalse);
    });
  });

  group('launching now', () {
    test('a launch keeps what the host answered for the card', () async {
      final host = ScriptedTransport({
        'GET /v1/schedules': '{"ok":true,"schedules":[$_active]}',
        'POST /v1/schedules/sch_1/run_now':
            '{"ok":true,"run":{"id":"srn_1","schedule_id":"sch_1",'
            '"due_at":"2026-11-12T08:00:00.000Z","started_at":null,'
            '"outcome":"skipped_cap","delegation_id":null}}',
      });
      final controller = _controller(host);
      addTearDown(controller.dispose);
      await controller.refresh();

      await controller.runNow(controller.schedules.single);

      expect(controller.launchOf('sch_1')?.skipped, RunOutcome.skippedCap);
      expect(host.calls, contains('POST /v1/schedules/sch_1/run_now'));
    });
  });
}
