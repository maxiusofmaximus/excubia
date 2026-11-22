import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/alerts/alert_status.dart';

void main() {
  test('reads the autostart and boot flags when the phone sends them', () {
    final status = AlertStatus.fromMap({
      'autostartScreen': true,
      'bootMissed': true,
    });

    expect(status.autostartScreen, isTrue);
    expect(status.bootMissed, isTrue);
  });

  test('a phone that does not send them counts as false', () {
    final status = AlertStatus.fromMap({});

    expect(status.autostartScreen, isFalse);
    expect(status.bootMissed, isFalse);
    expect(AlertStatus.unknown.autostartScreen, isFalse);
    expect(AlertStatus.unknown.bootMissed, isFalse);
  });
}
