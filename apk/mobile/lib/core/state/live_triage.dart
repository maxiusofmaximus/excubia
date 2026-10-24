import 'dart:async';
import 'dart:io';
import 'dart:math';

import 'package:flutter/foundation.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart' show ChatEvent;
import 'package:herdr_notify/src/rust/api/error.dart';
import 'package:herdr_notify/src/rust/api/catalog.dart';
import 'package:herdr_notify/src/rust/api/state.dart';
import 'package:herdr_notify/src/rust/api/stream.dart';
import 'package:herdr_notify/src/rust/api/workspace.dart';
import 'package:herdr_notify/src/rust/api/write.dart';

import '../api/gateway_client.dart';
import '../model/connection_state.dart';
import 'reconciler.dart';

/// Keeps one signed SSE stream open and publishes what it says.
///
/// The stream is authenticated once at connect; a reconnect is a fresh signed
/// request with a fresh nonce, never a resumption. Framing, JSON and the
/// `state_change_seq` continuity live in Rust; this class owns the socket loop,
/// the backoff and the UI state.
class LiveTriage extends ChangeNotifier {
  LiveTriage(
    this._client, {
    Random? random,
    this.baseBackoffMillis = 1000,
    this.maxBackoffMillis = 30000,
  }) : _random = random ?? Random();

  static const String reconnectReason = 'el equipo cerró la conexión';

  final GatewayClient _client;
  final Random _random;
  final int baseBackoffMillis;
  final int maxBackoffMillis;

  final StateReconciler reconciler = StateReconciler();
  final StreamController<ChatEvent> _chatSignals =
      StreamController<ChatEvent>.broadcast();

  ConnectionState _connection = const Connecting();
  DateTime? _lastGoodAt;
  bool _running = false;
  bool _stopped = false;

  ConnectionState get connection => _connection;

  StateSnapshot? get currentState => reconciler.lastGood;

  /// `chat.*` frames from the one stream. A chat frame never replaces the
  /// state read model, so it is published here instead of triggering a
  /// `/v1/state` poll.
  Stream<ChatEvent> get chatSignals => _chatSignals.stream;

  void start() {
    if (_running) return;
    _running = true;
    _stopped = false;
    unawaited(_run());
  }

  /// Stop the loop at its next observation point; the in-flight backoff delay
  /// is not cancellable, so this never blocks the caller.
  void stop() {
    _stopped = true;
    _running = false;
  }

  /// Read the full state now, rather than waiting for the next frame.
  Future<void> refreshNow() async {
    try {
      await _refresh();
    } on GatewayError catch (e) {
      final info = _explain(e);
      if (info.kind != FailureKind.retry) {
        _publish(info);
        stop();
      }
    }
  }

  /// Exponential backoff, capped, with jitter bounded to 20% so a test can
  /// assert a range. The spread is not decoration: deterministic backoff makes
  /// every phone on a tailnet reconnect in the same instant after a wake-up.
  int backoffMillis(int attempt) {
    final exponent = (attempt - 1).clamp(0, 5);
    final base = (baseBackoffMillis * (1 << exponent)).clamp(
      baseBackoffMillis,
      maxBackoffMillis,
    );
    final spread = base ~/ 5;
    final jitter = _random.nextInt(spread * 2 + 1) - spread;
    return (base + jitter).clamp(baseBackoffMillis, maxBackoffMillis);
  }

  Future<void> _run() async {
    var attempt = 0;
    while (!_stopped) {
      try {
        if (attempt == 0) {
          _set(const Connecting());
        }
        await _consume();
        _set(
          Offline(
            reconciler.lastGood,
            reason: reconnectReason,
            updatedAt: _lastGoodAt,
          ),
        );
        attempt = 0;
      } on GatewayError catch (e) {
        final info = _explain(e);
        if (info.kind != FailureKind.retry) {
          _publish(info);
          return;
        }
        _set(
          Offline(
            reconciler.lastGood,
            reason: info.message,
            updatedAt: _lastGoodAt,
          ),
        );
      } on IOException catch (e) {
        _set(
          Offline(
            reconciler.lastGood,
            reason: _explainTransport(e),
            updatedAt: _lastGoodAt,
          ),
        );
      } catch (e) {
        _set(Failed(e.toString(), kind: FailureKind.retry));
      }

      attempt++;
      await Future<void>.delayed(
        Duration(milliseconds: backoffMillis(attempt)),
      );
    }
  }

  Future<void> _consume() async {
    final stream = await _client.eventLines();
    final decoder = newStreamDecoder();
    await for (final line in stream) {
      if (_stopped) return;
      final event = decoder.pushLine(line: line);
      if (event != null) await _onFrame(event);
    }
  }

  Future<void> _onFrame(StreamEvent event) async {
    final chat = event.chat;
    if (chat != null) {
      _chatSignals.add(chat);
      return;
    }
    if (event.kind == StreamEventKind.state) {
      final state = event.state;
      if (state != null && reconciler.acceptState(state)) {
        _lastGoodAt = DateTime.now();
        _setLive();
      }
      return;
    }
    // A sequence hole means the transport dropped something; re-read even when
    // the frame's own revision looks stale.
    if (!reconciler.shouldRefreshForFrame(event.revision) && !event.seqGap) {
      return;
    }
    await _refresh();
  }

  Future<void> _refresh() async {
    final StateSnapshot? next;
    try {
      next = parseState(json: await _client.state());
    } on GatewayError catch (e) {
      if (e.status == 401 || e.status == 403) rethrow;
      return;
    }
    if (next == null || !reconciler.acceptState(next)) return;
    _lastGoodAt = DateTime.now();
    _setLive();
  }

  /// Run one write action. Returns the error to render, or null on success.
  ///
  /// The route, body and local validation come from the Rust plan; the HTTP
  /// call only carries the signed bytes.
  Future<ErrorInfo?> write(AgentAction action) async {
    final plan = planWrite(action: action);
    if (plan.problem != null) return plan.problem;
    final call = plan.request;
    if (call == null) {
      return const ErrorInfo(
        message: 'La petición no es válida.',
        kind: FailureKind.retry,
      );
    }
    try {
      await _client.call(call.method, call.target, call.body);
      unawaited(refreshNow());
      return null;
    } on GatewayError catch (e) {
      return _explainWrite(e, action.kind);
    } on Exception catch (e) {
      return ErrorInfo(message: e.toString(), kind: FailureKind.retry);
    }
  }

  ErrorInfo _explainWrite(GatewayError error, AgentActionKind kind) =>
      explainHttpError(
        status: error.status,
        body: error.body,
        action: switch (kind) {
          AgentActionKind.respond => WriteAction.respond,
          AgentActionKind.keys => WriteAction.keys,
          AgentActionKind.seen => WriteAction.seen,
        },
      );

  /// The host's closed catalogue. Only `available` entries are offered by the
  /// screen; this method returns everything it was given.
  Future<({List<CatalogEntry>? entries, ErrorInfo? error})>
  loadCatalog() async {
    try {
      final body = await _client.call('GET', '/v1/catalog');
      final entries = parseCatalog(json: body);
      if (entries == null) {
        return (
          entries: null,
          error: const ErrorInfo(
            message: 'Respuesta inesperada del equipo.',
            kind: FailureKind.retry,
          ),
        );
      }
      return (entries: entries, error: null);
    } on GatewayError catch (e) {
      return (
        entries: null,
        error: explainHttpError(
          status: e.status,
          body: e.body,
          action: WriteAction.read,
        ),
      );
    } on Exception catch (e) {
      return (
        entries: null,
        error: ErrorInfo(message: e.toString(), kind: FailureKind.retry),
      );
    }
  }

  /// Start one agent on the host. The route and body are planned in Rust.
  Future<({CreatedWorkspace? created, ErrorInfo? error})> createConversation({
    required String agent,
    required String cwd,
    String? label,
  }) async {
    final plan = planCreate(agent: agent, cwd: cwd, label: label);
    final error = plan.problem;
    if (error != null) return (created: null, error: error);
    final call = plan.request;
    if (call == null) {
      return (
        created: null,
        error: const ErrorInfo(
          message: 'La petición no es válida.',
          kind: FailureKind.retry,
        ),
      );
    }
    try {
      final body = await _client.call(call.method, call.target, call.body);
      final created = parseCreated(json: body);
      if (created == null) {
        return (
          created: null,
          error: const ErrorInfo(
            message: 'El equipo respondió algo que no es una conversación.',
            kind: FailureKind.retry,
          ),
        );
      }
      unawaited(refreshNow());
      return (created: created, error: null);
    } on GatewayError catch (e) {
      return (
        created: null,
        error: explainCreateError(status: e.status, body: e.body),
      );
    } on Exception catch (e) {
      return (
        created: null,
        error: ErrorInfo(message: e.toString(), kind: FailureKind.retry),
      );
    }
  }

  /// Stop one live agent. `gone` means the host answered `pane_gone`: the
  /// screen should leave with "ya había terminado", not an error.
  Future<({bool stopped, bool gone, ErrorInfo? error})> stopAgent(
    String paneId,
  ) async {
    final plan = planStop(paneId: paneId);
    final problem = plan.problem;
    if (problem != null) return (stopped: false, gone: false, error: problem);
    final call = plan.request;
    if (call == null) {
      return (
        stopped: false,
        gone: false,
        error: const ErrorInfo(
          message: 'La petición no es válida.',
          kind: FailureKind.retry,
        ),
      );
    }
    try {
      await _client.call(call.method, call.target, call.body);
      unawaited(refreshNow());
      return (stopped: true, gone: false, error: null);
    } on GatewayError catch (e) {
      final failure = classifyStopError(status: e.status, body: e.body);
      return (stopped: false, gone: failure.gone, error: failure.error);
    } on Exception catch (e) {
      return (
        stopped: false,
        gone: false,
        error: ErrorInfo(message: e.toString(), kind: FailureKind.retry),
      );
    }
  }

  ErrorInfo _explain(GatewayError error) => explainHttpError(
    status: error.status,
    body: error.body,
    action: WriteAction.read,
  );

  String _explainTransport(IOException error) => explainError(
    status: 0,
    message: error.toString(),
    action: WriteAction.read,
  ).message;

  void _setLive() {
    final state = reconciler.lastGood;
    if (state == null) return;
    _set(Live(state, updatedAt: _lastGoodAt ?? DateTime.now()));
  }

  void _publish(ErrorInfo info) =>
      _set(Failed(info.message, action: info.action, kind: info.kind));

  void _set(ConnectionState next) {
    // After stop()/dispose() nothing more is published: a late frame must not
    // touch a screen that is already gone.
    if (_stopped) return;
    _connection = next;
    notifyListeners();
  }

  @override
  void dispose() {
    _stopped = true;
    unawaited(_chatSignals.close());
    super.dispose();
  }
}
