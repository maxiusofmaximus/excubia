import 'package:herdr_notify/src/rust/api/stream.dart';

import '../api/gateway_client.dart';
import 'alert_policy.dart';

/// The two calls the alerts stream makes. [GatewayAlertSource] is the app's signed client.
abstract interface class AlertSource {
  Future<Stream<String>> lines({String? lastEventId});

  Future<String> state();
}

class GatewayAlertSource implements AlertSource {
  GatewayAlertSource(this._client);

  final GatewayClient _client;

  @override
  Future<Stream<String>> lines({String? lastEventId}) =>
      _client.eventLines(lastEventId: lastEventId);

  @override
  Future<String> state() => _client.state();
}

/// The Rust decoder keeps the framing rules; the alerts keep only what they act on. An `alert`
/// frame has no chat body, so its kind comes from its [dataLine].
AlertFrame? alertFrameOf(StreamEvent event, String dataLine) {
  if (event.event == 'alert') {
    return AlertFrame(event: 'alert', alert: hostAlertKindOf(dataLine));
  }
  final chat = event.chat;
  if (chat == null) return null;
  return AlertFrame(event: chat.event, status: chat.status);
}
