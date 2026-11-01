import 'package:flutter/services.dart';

import 'alert_status.dart';

/// What the alerts ask of the phone. Tests use a fake; the app uses [MethodChannelAlertPlatform].
abstract interface class AlertPlatform {
  Future<AlertStatus> status();

  Future<void> setEnabled(bool enabled);

  Future<void> openBatterySettings();

  Future<void> openChannelSettings(String channel);

  Future<void> openAppSettings();

  /// True when the phone's autostart screen opened; false when the app's own settings did.
  Future<bool> openAutostartSettings();

  Future<void> requestNotificationPermission();

  /// False when the UI is in front and the alert was left out.
  Future<bool> notify({
    required String channel,
    required String title,
    required String body,
  });

  Future<void> connection({required bool connected});

  Future<void> record(String line);

  Future<String> readLog();

  /// The service calls [handler] when the network comes back.
  void onNetworkUp(void Function() handler);
}

/// `dev.herdrnotify/alerts` (HerdrAlertsPlugin.kt).
class MethodChannelAlertPlatform implements AlertPlatform {
  static const MethodChannel _channel = MethodChannel('dev.herdrnotify/alerts');

  const MethodChannelAlertPlatform();

  @override
  Future<AlertStatus> status() async {
    final map = await _channel.invokeMapMethod<String, Object?>('status');
    return AlertStatus.fromMap(map ?? const {});
  }

  @override
  Future<void> setEnabled(bool enabled) =>
      _channel.invokeMethod<void>('setEnabled', {'enabled': enabled});

  @override
  Future<void> openBatterySettings() =>
      _channel.invokeMethod<void>('openBatterySettings');

  @override
  Future<void> openChannelSettings(String channel) =>
      _channel.invokeMethod<void>('openChannelSettings', {'channel': channel});

  @override
  Future<void> openAppSettings() => _channel.invokeMethod<void>('openAppSettings');

  @override
  Future<bool> openAutostartSettings() async =>
      await _channel.invokeMethod<bool>('openAutostartSettings') ?? false;

  @override
  Future<void> requestNotificationPermission() =>
      _channel.invokeMethod<void>('requestNotificationPermission');

  @override
  Future<bool> notify({
    required String channel,
    required String title,
    required String body,
  }) async {
    final shown = await _channel.invokeMethod<bool>('notify', {
      'channel': channel,
      'title': title,
      'body': body,
    });
    return shown ?? false;
  }

  @override
  Future<void> connection({required bool connected}) =>
      _channel.invokeMethod<void>('connection', {'connected': connected});

  @override
  Future<void> record(String line) =>
      _channel.invokeMethod<void>('record', {'line': line});

  @override
  Future<String> readLog() async =>
      await _channel.invokeMethod<String>('readLog') ?? '';

  @override
  void onNetworkUp(void Function() handler) {
    _channel.setMethodCallHandler((call) async {
      if (call.method == 'networkUp') handler();
    });
  }
}
