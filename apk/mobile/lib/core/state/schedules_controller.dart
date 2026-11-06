import 'dart:async';

import '../api/host_client.dart';
import '../api/schedule_models.dart';
import '../api/schedules_api.dart';
import 'active_poll_controller.dart';

/// «Automatizar»: the scheduled tasks, read with the team's own poll. A pause, a resume or a
/// launch answers with the host's schedule, which replaces the one on screen at once.
class SchedulesController extends ActivePollController {
  SchedulesController({required this.api, super.interval = kPollInterval});

  final SchedulesApi api;

  List<Schedule> _schedules = const <Schedule>[];
  Object? _error;
  bool _loaded = false;

  /// True once the host has refused a change for lack of `APPROVE`; lifted when the screen
  /// shows again, since the user may have been granted it meanwhile.
  bool _denied = false;

  final Set<String> _busy = <String>{};
  final Map<String, Object> _failures = <String, Object>{};
  final Map<String, ScheduleLaunch> _launches = <String, ScheduleLaunch>{};

  /// The tasks as the host sorted them: active first, then paused, then done.
  List<Schedule> get schedules => _schedules;

  /// The last read's failure; null once a read has answered.
  Object? get error => _error;

  /// True after the first answer, even an empty one.
  bool get loaded => _loaded;

  bool get denied => _denied;

  bool isBusy(String scheduleId) => _busy.contains(scheduleId);

  /// The failure of the last change to this task, or null.
  Object? failureOf(String scheduleId) => _failures[scheduleId];

  /// What the last «Lanzar ahora» of this task answered, or null.
  ScheduleLaunch? launchOf(String scheduleId) => _launches[scheduleId];

  @override
  void setActive(bool on) {
    if (on) _denied = false;
    super.setActive(on);
    if (on) emit();
  }

  @override
  Future<void> read() async {
    try {
      _schedules = await api.schedules();
      _error = null;
      _loaded = true;
    } catch (error) {
      _error = error;
    }
  }

  Future<void> changeStatus(Schedule schedule, ScheduleChange change) =>
      _act(schedule.id, () async {
        final updated = switch (change) {
          ScheduleChange.pause => await api.pause(schedule.id),
          ScheduleChange.resume => await api.resume(schedule.id),
        };
        _replace(updated);
      });

  /// Launches the task now and reads the list again, since its last run may have changed.
  Future<void> runNow(Schedule schedule) => _act(schedule.id, () async {
    _launches[schedule.id] = await api.runNow(schedule.id);
    unawaited(refresh());
  });

  Future<void> _act(String scheduleId, Future<void> Function() change) async {
    if (_denied || _busy.contains(scheduleId)) return;
    _busy.add(scheduleId);
    _failures.remove(scheduleId);
    _launches.remove(scheduleId);
    emit();
    try {
      await change();
    } catch (error) {
      _fail(scheduleId, error);
    } finally {
      _busy.remove(scheduleId);
      emit();
    }
  }

  void _replace(Schedule updated) {
    _schedules = [
      for (final schedule in _schedules)
        schedule.id == updated.id ? updated : schedule,
    ];
  }

  /// A refusal that is not about the task itself is kept on the task; the list is read again.
  void _fail(String scheduleId, Object error) {
    if (error is HostError && error.status == 403) {
      _denied = true;
      return;
    }
    _failures[scheduleId] = error;
    if (error is HostError && (error.status == 404 || error.status == 409)) {
      unawaited(refresh());
    }
  }
}
