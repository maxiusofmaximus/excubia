import 'dart:async';

import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/state/live_triage.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';

/// Lets a test push chat frames into the controller, which the SSE socket
/// would otherwise feed.
class SignalTriage extends LiveTriage {
  SignalTriage()
    : super(
        HostClient(
          baseUrl: 'http://100.64.0.1:8788',
          deviceId: 'dev-x',
          privateKeyBase64: 'placeholder',
        ),
      );

  final StreamController<ChatEvent> signals =
      StreamController<ChatEvent>.broadcast();

  @override
  Stream<ChatEvent> get chatSignals => signals.stream;
}
