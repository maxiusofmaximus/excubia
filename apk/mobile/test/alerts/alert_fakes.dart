import 'dart:async';

import 'package:excubia/core/alerts/alert_platform.dart';
import 'package:excubia/core/alerts/alert_policy.dart';
import 'package:excubia/core/alerts/alert_source.dart';
import 'package:excubia/core/alerts/alert_status.dart';

/// Records what the alerts ask of the phone. Tests set the status it reports.
class FakeAlertPlatform implements AlertPlatform {
  AlertStatus reported = AlertStatus.unknown;
  bool notifyShown = true;
  String log = '';
  final List<({String channel, String title, String body})> notices = [];
  final List<bool> connections = [];
  final List<String> records = [];
  final List<bool> enabledCalls = [];
  final List<String> opened = [];
  void Function()? _onNetworkUp;

  @override
  Future<AlertStatus> status() async => reported;

  @override
  Future<void> setEnabled(bool enabled) async => enabledCalls.add(enabled);

  @override
  Future<void> openBatterySettings() async => opened.add('battery');

  @override
  Future<void> openChannelSettings(String channel) async =>
      opened.add('channel:$channel');

  @override
  Future<void> openAppSettings() async => opened.add('app');

  @override
  Future<bool> openAutostartSettings() async {
    opened.add('autostart');
    return true;
  }

  @override
  Future<void> requestNotificationPermission() async =>
      opened.add('permission');

  @override
  Future<bool> notify({
    required String channel,
    required String title,
    required String body,
  }) async {
    notices.add((channel: channel, title: title, body: body));
    return notifyShown;
  }

  @override
  Future<void> connection({required bool connected}) async =>
      connections.add(connected);

  @override
  Future<void> record(String line) async => records.add(line);

  @override
  Future<String> readLog() async => log;

  @override
  void onNetworkUp(void Function() handler) => _onNetworkUp = handler;

  void networkUp() => _onNetworkUp?.call();
}

/// One plan per connection: a stream of lines, or an error thrown when connecting.
/// Past the last plan the connection stays open and silent.
class FakeAlertSource implements AlertSource {
  FakeAlertSource(this.plans);

  final List<Object> plans;
  final List<String?> resumedFrom = [];
  String stateJson = '{}';
  Object? stateError;
  int stateCalls = 0;

  @override
  Future<Stream<String>> lines({String? lastEventId}) async {
    resumedFrom.add(lastEventId);
    if (plans.isEmpty) return StreamController<String>().stream;
    final plan = plans.removeAt(0);
    if (plan is Stream<String>) return plan;
    throw plan;
  }

  @override
  Future<String> state() async {
    stateCalls++;
    final error = stateError;
    if (error != null) throw error;
    return stateJson;
  }
}

/// Reads one `x` line as an approval, `y` as a finished task, `b`, `d` and `r` as the
/// host's blocked, done and reminder alerts, `u` as an alert of an unknown kind, and
/// anything else as nothing.
AlertDecode scriptedDecode() => (line) => switch (line) {
  'x' => const AlertFrame(event: 'chat.approval'),
  'y' => const AlertFrame(event: 'chat.delegation', status: 'done'),
  'b' => const AlertFrame(event: 'alert', alert: AlertKind.blocked),
  'd' => const AlertFrame(event: 'alert', alert: AlertKind.done),
  'r' => const AlertFrame(event: 'alert', alert: AlertKind.reminder),
  'u' => const AlertFrame(event: 'alert'),
  _ => null,
};

/// Waits in small steps until [condition] holds, so a test fails with a clear reason.
Future<void> until(bool Function() condition) async {
  for (var attempt = 0; attempt < 200; attempt++) {
    if (condition()) return;
    await Future<void>.delayed(const Duration(milliseconds: 5));
  }
  throw StateError('condition not reached in time');
}
