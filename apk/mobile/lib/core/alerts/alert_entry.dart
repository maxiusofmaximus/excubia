import 'package:herdr_notify/src/rust/api/stream.dart';

import '../api/host_client.dart';
import '../storage/pairing_store.dart';
import 'alert_platform.dart';
import 'alert_policy.dart';
import 'alert_source.dart';
import 'alert_stream.dart';

/// Flag the alerts service passes to its background engine (AlertService.kt).
const String alertsBackgroundArg = '--herdr-alerts-bg';

/// The alerts engine: reads the pairing and keeps the stream open until the service stops.
Future<void> runAlertStream() async {
  final platform = MethodChannelAlertPlatform();
  final pairing = await PairingStore(SecureKeyValueStore()).load();
  if (!pairing.isComplete) {
    await platform.record('sin emparejamiento: no hay nada que escuchar');
    return;
  }
  final stream = AlertStream(
    source: HostAlertSource(HostClient.forPairing(pairing)),
    platform: platform,
    newDecoder: _decoderOf,
  );
  platform.onNetworkUp(stream.networkUp);
  await stream.run();
}

AlertDecode _decoderOf() {
  final decoder = newStreamDecoder();
  // The `data:` line of the frame being read: an `alert` frame's kind is only there.
  var data = '';
  return (line) {
    if (line.startsWith('data:')) data = line;
    final event = decoder.pushLine(line: line);
    if (event == null) return null;
    final frame = alertFrameOf(event, data);
    data = '';
    return frame;
  };
}
