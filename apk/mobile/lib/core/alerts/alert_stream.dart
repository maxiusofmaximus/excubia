import 'dart:async';
import 'dart:io';

import '../api/gateway_client.dart';
import 'alert_notice.dart';
import 'alert_platform.dart';
import 'alert_policy.dart';
import 'alert_source.dart';

/// Keeps the signed stream open for the alerts service and alerts on approvals, finished
/// tasks and the host's own alerts. A lost stream is retried with [alertBackoff], resumed with
/// `Last-Event-ID`, and replaced when silent for [heartbeat]. A lost network never stops it:
/// the notice says so.
class AlertStream {
  AlertStream({
    required this.source,
    required this.platform,
    required this.newDecoder,
    this.heartbeat = alertHeartbeat,
    Future<void> Function(Duration)? delay,
  }) : _delay = delay ?? ((wait) => Future<void>.delayed(wait));

  final AlertSource source;
  final AlertPlatform platform;
  final AlertDecoderFactory newDecoder;
  final Duration heartbeat;
  final Future<void> Function(Duration) _delay;

  String? _lastEventId;
  Completer<void>? _wake;
  bool _networkBack = false;
  bool _hostDecides = false;

  /// Never returns: the service ends the engine when it stops.
  Future<void> run() async {
    var attempt = 0;
    while (true) {
      final result = await _connectOnce();
      // A network that just came back may not route yet: retry on the short steps again.
      attempt = result.reached ? 1 : (_networkBack ? 1 : attempt + 1);
      _networkBack = false;
      await platform.connection(connected: false);
      final wait = alertBackoff(attempt);
      await platform.record(
        'reconexión: ${result.reason}; nuevo intento en ${wait.inSeconds} s',
      );
      await _pause(wait);
    }
  }

  /// The network is back: skip the wait and connect now.
  void networkUp() {
    final wake = _wake;
    if (wake != null && !wake.isCompleted) {
      _networkBack = true;
      wake.complete();
    }
  }

  Future<_Attempt> _connectOnce() async {
    try {
      final lines = await source.lines(lastEventId: _lastEventId);
      await platform.connection(connected: true);
      await _learnHost();
      final decode = newDecoder();
      await for (final line in lines.timeout(heartbeat, onTimeout: _silent)) {
        _track(line);
        final frame = decode(line);
        if (frame != null) await _onFrame(frame);
      }
      return const _Attempt(reached: true, reason: 'el PC cerró la conexión');
    } on _Silence {
      return _Attempt(
        reached: true,
        reason: 'sin datos del PC en ${heartbeat.inSeconds} s',
      );
    } on GatewayError catch (e) {
      return _Attempt(reached: false, reason: _why(e));
    } on IOException {
      return const _Attempt(reached: false, reason: 'sin red ni ruta al PC');
    } on Object catch (e) {
      return _Attempt(reached: false, reason: 'error ${e.runtimeType}');
    }
  }

  void _track(String line) {
    final id = sseIdOf(line);
    if (id != null) _lastEventId = id.isEmpty ? null : id;
  }

  /// A host with alert rules decides which changes alert. A state that cannot be read keeps
  /// the answer of the last connection.
  Future<void> _learnHost() async {
    final json = await _state();
    if (json != null) _hostDecides = hostDecidesAlerts(json);
  }

  Future<void> _onFrame(AlertFrame frame) async {
    final kind = alertKindOf(frame, hostDecides: _hostDecides);
    if (kind == null) return;
    await platform.record('evento recibido: ${frame.event}');
    final notice = switch (kind) {
      AlertKind.approval => await _approvalNotice(),
      AlertKind.review => AlertNotice.generic,
      AlertKind.blocked => AlertNotice.alert('Un agente necesita tu atención'),
      AlertKind.done => AlertNotice.alert('Un agente ha terminado'),
      AlertKind.reminder => AlertNotice.alert(
        'Tienes un recordatorio del calendario',
      ),
    };
    if (notice == null) return;
    final shown = await platform.notify(
      channel: kind.channel,
      title: notice.title,
      body: notice.body,
    );
    await platform.record(
      shown
          ? 'notificado: ${kind.channel}'
          : 'omitido: la app está en primer plano',
    );
  }

  /// The approvals notice from the state; null when the state is unreadable or the approval
  /// was answered before the read, so nothing is left to act on.
  Future<AlertNotice?> _approvalNotice() async {
    final json = await _state();
    if (json == null) return null;
    final notice = AlertNotice.fromState(json);
    if (identical(notice, AlertNotice.generic)) {
      await platform.record('sin aviso: no hay aprobaciones pendientes');
      return null;
    }
    return notice;
  }

  /// The state, or null when it cannot be read; the reason goes to the log.
  Future<String?> _state() async {
    try {
      return await source.state();
    } on GatewayError catch (e) {
      await platform.record('sin estado: ${_why(e)}');
    } on IOException {
      await platform.record('sin estado: sin red');
    }
    return null;
  }

  Future<void> _pause(Duration wait) {
    final wake = Completer<void>();
    _wake = wake;
    return Future.any([_delay(wait), wake.future]);
  }

  static void _silent(EventSink<String> sink) {
    sink.addError(const _Silence());
    sink.close();
  }
}

class _Silence implements Exception {
  const _Silence();
}

class _Attempt {
  const _Attempt({required this.reached, required this.reason});

  /// The stream opened at least once, so the next wait starts again from the first step.
  final bool reached;
  final String reason;
}

/// Why a request failed: the status, or for a transport failure (status 0) the kind of failure.
String _why(GatewayError e) => e.status == 0
    ? 'transporte: ${e.failure?.name ?? 'desconocido'}'
    : 'HTTP ${e.status}';
