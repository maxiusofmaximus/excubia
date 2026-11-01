/// What the setup screen shows: the phone's answers to the alerts' requirements.
class AlertStatus {
  const AlertStatus({
    required this.notificationsGranted,
    required this.channels,
    required this.batteryExempt,
    required this.serviceRunning,
    required this.enabled,
    required this.lastExit,
    required this.manufacturer,
    required this.fgsType,
    required this.autostartScreen,
    required this.bootMissed,
  });

  /// Before the first answer from the phone.
  static const AlertStatus unknown = AlertStatus(
    notificationsGranted: false,
    channels: {},
    batteryExempt: false,
    serviceRunning: false,
    enabled: false,
    lastExit: 'sin datos',
    manufacturer: '',
    fgsType: '',
    autostartScreen: false,
    bootMissed: false,
  );

  factory AlertStatus.fromMap(Map<String, Object?> map) {
    final channels = map['channels'];
    return AlertStatus(
      notificationsGranted: map['notificationsGranted'] == true,
      channels: channels is Map
          ? {
              for (final entry in channels.entries)
                entry.key.toString(): entry.value == true,
            }
          : const {},
      batteryExempt: map['batteryExempt'] == true,
      serviceRunning: map['serviceRunning'] == true,
      enabled: map['enabled'] == true,
      lastExit: map['lastExit'] as String? ?? 'sin datos',
      manufacturer: map['manufacturer'] as String? ?? '',
      fgsType: map['fgsType'] as String? ?? '',
      autostartScreen: map['autostartScreen'] == true,
      bootMissed: map['bootMissed'] == true,
    );
  }

  final bool notificationsGranted;

  /// Channel id → whether the user left it on.
  final Map<String, bool> channels;
  final bool batteryExempt;
  final bool serviceRunning;

  /// The saved switch, which can be on while the service is down (the watchdog brings it back).
  final bool enabled;
  final String lastExit;

  /// Lower case, as `Build.MANUFACTURER` gives it.
  final String manufacturer;

  /// `specialUse`, `connectedDevice` or `dataSync`: the type this build measures.
  final String fgsType;

  /// Phone Master (TECNO) has its autostart screen, so the app can open it.
  final bool autostartScreen;

  /// The switch is on, the phone is unlocked and the last boot did not restart the service.
  final bool bootMissed;
}
