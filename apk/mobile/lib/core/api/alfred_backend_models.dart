import 'dart:convert';

import 'json_read.dart';

/// Where Alfred's turns are answered now, or on the way there (`/v1/alfred/backend`).
/// [engine]: the turns run on the CLI named by `backend`, which the PC lists as ready.
enum AlfredEngineState { claude, starting, local, engine, stopping, failed }

/// A CLI engine the PC lists as ready (`engines` in the status). It can be chosen.
class AlfredCliEngine {
  const AlfredCliEngine({required this.name, required this.version});

  final String name;
  final String version;
}

/// The engine of Alfred as the PC reports it. [backend] is the engine the turns use now;
/// [state] adds the change under way. Read tolerantly: unknown fields are ignored and an
/// unknown [state] reads as Claude, keeping the `detail` the PC sent. A PC that predates
/// [engines] reads as none listed.
class AlfredBackendStatus {
  const AlfredBackendStatus({
    required this.backend,
    required this.state,
    this.detail,
    this.since,
    this.voiceSuspended = false,
    this.engines = const [],
  });

  final String backend;
  final AlfredEngineState state;
  final String? detail;
  final int? since;
  final bool voiceSuspended;
  final List<AlfredCliEngine> engines;

  factory AlfredBackendStatus.fromJson(Map<String, Object?> json) {
    final name = json['state'];
    final state = AlfredEngineState.values.asNameMap()[name];
    final detail = json['detail'];
    final since = json['since'];
    final engines = json['engines'];
    return AlfredBackendStatus(
      backend: json['backend'] is String ? json['backend'] as String : 'claude',
      state: state ?? AlfredEngineState.claude,
      detail: detail is String ? detail : null,
      since: since is int ? since : null,
      voiceSuspended: json['voice_suspended'] == true,
      engines: engines is List
          ? [
              for (final item in engines)
                if (item is Map &&
                    item['name'] is String &&
                    item['version'] is String)
                  AlfredCliEngine(
                    name: item['name'] as String,
                    version: item['version'] as String,
                  ),
            ]
          : const [],
    );
  }
}

/// The status answer of the PC. Its body is a plain object, without the `ok` envelope.
AlfredBackendStatus parseAlfredBackendStatus(String body) =>
    AlfredBackendStatus.fromJson(jsonObject(jsonDecode(body)));
