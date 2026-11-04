import 'dart:convert';

import 'json_read.dart';

/// Where Alfred's turns are answered now, or on the way there (`/v1/alfred/backend`).
enum AlfredEngineState { claude, starting, local, stopping, failed }

/// The engine of Alfred as the PC reports it. [backend] is the engine the turns use now;
/// [state] adds the change under way. Read tolerantly: unknown fields are ignored and an
/// unknown [state] reads as Claude, keeping the `detail` the PC sent.
class AlfredBackendStatus {
  const AlfredBackendStatus({
    required this.backend,
    required this.state,
    this.detail,
    this.since,
    this.voiceSuspended = false,
  });

  final String backend;
  final AlfredEngineState state;
  final String? detail;
  final int? since;
  final bool voiceSuspended;

  factory AlfredBackendStatus.fromJson(Map<String, Object?> json) {
    final name = json['state'];
    final state = AlfredEngineState.values.asNameMap()[name];
    final detail = json['detail'];
    final since = json['since'];
    return AlfredBackendStatus(
      backend: json['backend'] is String ? json['backend'] as String : 'claude',
      state: state ?? AlfredEngineState.claude,
      detail: detail is String ? detail : null,
      since: since is int ? since : null,
      voiceSuspended: json['voice_suspended'] == true,
    );
  }
}

/// The status answer of the PC. Its body is a plain object, without the `ok` envelope.
AlfredBackendStatus parseAlfredBackendStatus(String body) =>
    AlfredBackendStatus.fromJson(jsonObject(jsonDecode(body)));
