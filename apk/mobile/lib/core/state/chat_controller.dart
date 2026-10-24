import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';
import 'package:herdr_notify/src/rust/api/chat/plan.dart';
import 'package:herdr_notify/src/rust/api/chat/reducer.dart';
import 'package:herdr_notify/src/rust/api/error/chat.dart';
import 'package:herdr_notify/src/rust/api/error.dart'
    show FailureKind, WriteAction;
import 'package:herdr_notify/src/rust/api/state.dart' show AgentEntry;
import 'package:herdr_notify/src/rust/api/write.dart' show HttpCall;

import '../api/chat_transport.dart';
import '../api/gateway_client.dart';
import '../model/connection_state.dart';
import 'approvals_controller.dart';
import 'live_triage.dart';

part 'chat_controller_delegation.dart';
part 'chat_controller_history.dart';

/// Which slice of the history the drawer asks the host for.
enum HistoryFilter { all, pinned, sections, archived }

/// One refusal from the chat surface. `code` is stable; the screen branches on
/// it and never on a raw HTTP status.
typedef ChatProblem = ChatFailure;

/// Owns the chat read model: bootstrap, history, the current timeline and the
/// outgoing message states. Every route and body comes from Rust; this class
/// only sequences calls, caches the last good view and notifies the screen.
class ChatController extends ChangeNotifier {
  ChatController(
    this._transport,
    this._triage, {
    this.approvals,
    this.searchDelay = const Duration(milliseconds: 220),
    this.refreshDelay = const Duration(milliseconds: 180),
  }) : _reducer = newChatReducer();

  final ChatTransport _transport;
  final LiveTriage _triage;

  /// Approval state, fed from the same SSE stream; null in tests without it.
  final ApprovalsController? approvals;
  final Duration searchDelay;
  final Duration refreshDelay;
  final ChatReducer _reducer;

  final StreamController<ChatProblem> _failures =
      StreamController<ChatProblem>.broadcast();
  StreamSubscription<ChatEvent>? _chatSubscription;

  ChatBootstrap? _bootstrap;
  List<ChatConversation> _conversations = const <ChatConversation>[];
  List<ChatSection> _sections = const <ChatSection>[];
  ChatConversation? _current;
  ChatProblem? _failure;
  Timer? _searchTimer;
  Timer? _refreshTimer;
  Timer? _listTimer;
  DateTime? _lastSync;
  String? _query = '';
  HistoryFilter _filter = HistoryFilter.all;
  bool _loading = true;
  bool _loadingConversation = false;
  bool _loadingMore = false;
  bool _sending = false;
  bool _disposed = false;
  bool _wasLive = false;
  String? _retryClientId;
  int _pinnedCount = 0;
  int _archivedCount = 0;
  final Map<String, int> _readSent = <String, int>{};

  ChatBootstrap? get bootstrap => _bootstrap;
  List<ChatProject> get projects =>
      _bootstrap?.projects ?? const <ChatProject>[];
  LiveTriage get triage => _triage;
  List<ChatConversation> get conversations => _conversations;
  List<ChatSection> get sections => _sections;
  ChatConversation? get current => _current;
  ChatProblem? get failure => _failure;
  String get query => _query ?? '';
  HistoryFilter get filter => _filter;
  bool get loading => _loading;
  bool get loadingConversation => _loadingConversation;
  bool get loadingMore => _loadingMore;
  bool get sending => _sending;
  DateTime? get lastSync => _lastSync;
  int get pinnedCount => _pinnedCount;
  int get archivedCount => _archivedCount;
  String get alfredName => _bootstrap?.alfredName ?? 'Alfred';

  String get alfredStatus {
    final status = _bootstrap?.alfredStatus ?? 'draft';
    return switch (status) {
      'working' => 'Trabajando',
      'blocked' => 'Esperando',
      'stopped' => 'No disponible',
      'idle' => 'Disponible',
      'draft' => 'Disponible',
      _ => 'Disponible',
    };
  }

  bool get offline =>
      _triage.connection is Offline || _triage.connection is Failed;

  bool get canSend => !offline && !_sending;

  int get unreadMessages =>
      _conversations.fold(0, (total, chat) => total + chat.unread.toInt());

  int get pendingApprovals => _bootstrap?.pendingApprovals.toInt() ?? 0;

  List<AgentEntry> get teamAgents =>
      _triage.currentState?.agents ?? const <AgentEntry>[];

  List<ChatEntry> get entries => _reducer.snapshot().entries;

  bool get hasMore => _reducer.snapshot().hasMore;

  int get lastSeq => _reducer.snapshot().lastSeq;

  int? get oldestSeq => _reducer.snapshot().oldestSeq;

  ChatEntry? get failedLocal {
    for (final entry in entries.reversed) {
      if (entry.local && entry.state == 'failed') return entry;
    }
    return null;
  }

  ChatEntry? get failedTurn {
    for (final entry in entries.reversed) {
      if (!entry.local &&
          entry.kind == 'message' &&
          entry.state == 'failed' &&
          (entry.body.isEmpty || entry.authorKind == 'agent')) {
        return entry;
      }
    }
    return null;
  }

  Future<void> start() async {
    _chatSubscription = _triage.chatSignals.listen(_onChatEvent);
    _triage.addListener(_onTriageChanged);
    approvals?.addListener(_notify);
    await _loadBootstrap();
    await Future.wait([_loadSections(), _refreshConversations()]);
    _loading = false;
    _notify();
    if (_current == null && _conversations.isNotEmpty) {
      await openConversation(_conversations.first.id);
    }
  }

  Future<void> refresh() async {
    await Future.wait([
      _loadBootstrap(),
      _loadSections(),
      _refreshConversations(),
    ]);
    final id = _current?.id;
    if (id != null) await _loadConversation(id, reset: false);
    _notify();
  }

  void reconnect() {
    _triage.start();
    unawaited(_triage.refreshNow());
    unawaited(refresh());
  }

  Future<void> openConversation(String id) async {
    final known = _conversationFor(id);
    _current = known;
    _failure = null;
    _reducer.reset();
    _loadingConversation = true;
    _notify();
    await _loadConversation(id, reset: true);
  }

  Future<String?> createConversation({String? title}) async {
    final plan = planChatCreateConversation(title: title);
    try {
      final body = await _execute(plan);
      final created = parseChatConversation(json: body);
      if (created == null) {
        _failure = _localFailure(
          'El equipo no devolvió la conversación creada.',
        );
        _notify();
        return null;
      }
      _conversations = [created, ..._conversations];
      _current = created;
      _failure = null;
      _reducer.reset();
      _notify();
      return created.id;
    } catch (error) {
      _failure = _explain(error, WriteAction.respond);
      _notify();
      return null;
    }
  }

  /// Send one user turn. The conversation is created on the first message so
  /// the empty start screen needs no separate form.
  Future<void> send(String raw) async {
    final text = raw.trim();
    if (text.isEmpty) return;
    if (offline) {
      _failure = offlineChatFailure(
        message: 'Sin conexión con el equipo.',
        action: 'Reconecta para escribir a Alfred.',
      );
      _notify();
      return;
    }
    var id = _current?.id;
    if (id == null) {
      id = await createConversation(title: _titleFrom(text));
      if (id == null) return;
    }
    await _sendText(id, text, newChatClientId());
  }

  /// Retry a failed local bubble. A transport failure reuses the `client_id`
  /// because the host may have stored the message; an HTTP refusal did not
  /// store anything, so a fresh id avoids resurrecting a refused turn.
  Future<void> retryLocal(ChatEntry entry) async {
    final id = _current?.id;
    final old = entry.clientId;
    if (id == null || old == null) return;
    final reuse = _retryClientId == old;
    final clientId = reuse ? old : newChatClientId();
    _reducer.discardSend(clientId: old);
    _failure = null;
    await _sendText(id, entry.body, clientId);
  }

  /// Retry a failed turn: send the user message that preceded the empty agent
  /// failure card again, with a new `client_id` (the failed one is durable).
  Future<void> retryTurn(ChatEntry failed) async {
    final id = _current?.id;
    if (id == null) return;
    final all = entries;
    final index = all.indexWhere((entry) => entry.seq == failed.seq);
    for (var cursor = index - 1; cursor >= 0; cursor--) {
      final candidate = all[cursor];
      if (candidate.authorKind == 'user' && candidate.body.isNotEmpty) {
        await _sendText(id, candidate.body, newChatClientId());
        return;
      }
    }
  }

  Future<void> loadOlder() async {
    final id = _current?.id;
    final before = oldestSeq;
    if (_loadingMore || id == null || before == null || !hasMore) return;
    _loadingMore = true;
    _notify();
    try {
      final body = await _execute(
        planChatTimeline(conversationId: id, beforeSeq: before),
      );
      _reducer.acceptTimeline(json: body);
      _lastSync = DateTime.now();
    } catch (error) {
      _failure = _explain(error, WriteAction.read);
    } finally {
      _loadingMore = false;
      _notify();
    }
  }

  Stream<ChatProblem> get failures => _failures.stream;

  void clearFailure() {
    if (_failure == null) return;
    _failure = null;
    _notify();
  }

  /// The one recovery the failure card offers: resend the local turn, reconnect
  /// when offline, or read the state again.
  void retryFailure() {
    final local = failedLocal;
    if (local != null) {
      unawaited(retryLocal(local));
      return;
    }
    if (offline) {
      reconnect();
      return;
    }
    unawaited(refresh());
  }

  Future<void> _sendText(
    String conversationId,
    String text,
    String clientId,
  ) async {
    final plan = planChatSend(
      conversationId: conversationId,
      clientId: clientId,
      text: text,
    );
    _reducer.beginSend(clientId: clientId, text: text);
    _sending = true;
    _failure = null;
    _notify();
    var accepted = false;
    try {
      final body = await _execute(plan);
      accepted = true;
      final linked = _reducer.acceptSent(clientId: clientId, json: body);
      if (!linked) {
        _reducer.discardSend(clientId: clientId);
        _failure = _localFailure(
          'El equipo aceptó el mensaje pero no lo devolvió.',
        );
      }
      _retryClientId = null;
      _lastSync = DateTime.now();
      _markRead();
    } catch (error) {
      final failure = _explain(error, WriteAction.respond);
      _failure = failure;
      if (accepted || failure.keptByHost) {
        // The host kept the turn (or accepted it but failed the run); the
        // timeline is the truth, so the optimistic bubble is not duplicated.
        _reducer.discardSend(clientId: clientId);
        unawaited(_refreshCurrent());
      } else {
        _reducer.failSend(clientId: clientId);
      }
      _retryClientId = error is GatewayError && error.status == 0
          ? clientId
          : null;
    } finally {
      _sending = false;
      _notify();
      unawaited(_refreshConversations(silent: true));
    }
  }

  Future<void> _loadBootstrap() async {
    try {
      final body = await _execute(planChatBootstrap());
      final boot = parseChatBootstrap(json: body);
      if (boot != null) _bootstrap = boot;
      _lastSync = DateTime.now();
    } catch (_) {
      // The list and the timeline still work with a stale bootstrap; the
      // header falls back to its defaults instead of blanking the screen.
    }
  }

  Future<void> _loadConversation(String id, {required bool reset}) async {
    if (reset) {
      _reducer.reset();
      _readSent.remove(id);
    }
    try {
      final results = await Future.wait([
        _execute(planChatTimeline(conversationId: id)),
        _execute(planChatMessages(conversationId: id, afterSeq: 0)),
      ]);
      if (_current?.id != id) return;
      _reducer.acceptMessages(json: results[1]);
      _reducer.acceptTimeline(json: results[0]);
      _lastSync = DateTime.now();
      _failure = null;
      _markRead();
    } catch (error) {
      if (_current?.id == id) _failure = _explain(error, WriteAction.read);
    } finally {
      if (_current?.id == id) {
        _loadingConversation = false;
        _notify();
      }
    }
  }

  Future<void> _refreshCurrent() async {
    final id = _current?.id;
    if (id == null || _loadingConversation) return;
    final after = lastSeq;
    try {
      final results = await Future.wait([
        _execute(planChatTimeline(conversationId: id)),
        _execute(planChatMessages(conversationId: id, afterSeq: after)),
      ]);
      if (_current?.id != id) return;
      _reducer.acceptTimeline(json: results[0]);
      _reducer.acceptMessages(json: results[1]);
      _lastSync = DateTime.now();
      _markRead();
      _notify();
    } catch (_) {
      // A refresh never replaces a good view with a transient failure.
    }
  }

  Future<ChatProblem?> _action(ChatPlan plan) async {
    try {
      await _execute(plan);
      await _refreshConversations();
      await _loadCounts();
      if (_current != null) {
        final updated = _conversationFor(_current!.id);
        if (updated != null) _current = updated;
      }
      _notify();
      return null;
    } catch (error) {
      final failure = _explain(error, WriteAction.respond);
      _failures.add(failure);
      return failure;
    }
  }

  void _onChatEvent(ChatEvent event) {
    approvals?.onChatEvent(event);
    _reducer.acceptChatEvent(event: event);
    if (event.conversationId == _current?.id) {
      _refreshTimer?.cancel();
      _refreshTimer = Timer(refreshDelay, () => unawaited(_refreshCurrent()));
    }
    _listTimer?.cancel();
    _listTimer = Timer(
      refreshDelay,
      () => unawaited(_refreshConversations(silent: true)),
    );
    _notify();
  }

  void _onTriageChanged() {
    final live = _triage.connection is Live;
    if (live && !_wasLive && !_loading) {
      unawaited(refresh());
    }
    _wasLive = live;
    _notify();
  }

  void _markRead() {
    final id = _current?.id;
    final seq = lastSeq;
    if (id == null || seq <= 0) return;
    final sent = _readSent[id] ?? 0;
    if (seq <= sent) return;
    _readSent[id] = seq;
    unawaited(_markReadNow(id, seq));
  }

  Future<void> _markReadNow(String id, int seq) async {
    try {
      await _execute(planChatRead(conversationId: id, lastSeq: seq));
    } catch (_) {
      // The read cursor is best effort; the next open retries it.
    }
  }

  Future<String> _execute(ChatPlan plan) async {
    final problem = plan.problem;
    if (problem != null) {
      throw _LocalChatProblem(localChatFailure(error: problem));
    }
    final HttpCall? request = plan.request;
    if (request == null) {
      throw _LocalChatProblem(_localFailure('La petición no es válida.'));
    }
    return _transport.call(request.method, request.target, request.body);
  }

  ChatProblem _explain(Object error, WriteAction action) {
    if (error is _LocalChatProblem) return error.failure;
    if (error is GatewayError) {
      return explainChatError(
        status: error.status,
        body: error.body,
        retryAfter: error.retryAfter,
        action: action,
      );
    }
    return _localFailure(error.toString());
  }

  ChatProblem _localFailure(String message) => ChatProblem(
    code: 'local',
    message: message,
    action: 'Vuelve a intentarlo.',
    kind: FailureKind.retry,
    shownInTimeline: false,
    keptByHost: false,
  );

  ChatConversation? _conversationFor(String id) {
    for (final conversation in _conversations) {
      if (conversation.id == id) return conversation;
    }
    return null;
  }

  String _titleFrom(String text) {
    final line = text.split('\n').first.trim();
    return line.length <= 60 ? line : '${line.substring(0, 59)}…';
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    _searchTimer?.cancel();
    _refreshTimer?.cancel();
    _listTimer?.cancel();
    unawaited(_chatSubscription?.cancel());
    _triage.removeListener(_onTriageChanged);
    approvals?.removeListener(_notify);
    unawaited(_failures.close());
    super.dispose();
  }
}

class _LocalChatProblem implements Exception {
  const _LocalChatProblem(this.failure);

  final ChatProblem failure;
}
