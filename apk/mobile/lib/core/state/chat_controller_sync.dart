part of 'chat_controller.dart';

/// Reading the host into the model: bootstrap, the open conversation's timeline,
/// older pages, the live refresh and the read cursor.
extension ChatControllerSync on ChatController {
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
    _replyTo = null;
    _reducer.reset();
    _loadingConversation = true;
    _notify();
    await _loadConversation(id, reset: true);
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
      _reducer.settleTurn(conversationId: id);
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
      _reducer.settleTurn(conversationId: id);
      _lastSync = DateTime.now();
      _markRead();
      _notify();
    } catch (_) {
      // A refresh never replaces a good view with a transient failure.
    }
  }

  void _onChatEvent(ChatEvent event) {
    approvals?.onChatEvent(event);
    _reducer.acceptChatEvent(event: event);
    if (event.event == 'chat.conversation' && event.change == 'deleted') {
      _forgetConversation(event.conversationId);
      unawaited(_loadCounts());
    }
    if (event.conversationId == _current?.id) {
      _refreshTimer?.cancel();
      _refreshTimer = Timer(refreshDelay, () => unawaited(_refreshCurrent()));
    }
    _listTimer?.cancel();
    _listTimer = Timer(refreshDelay, () {
      unawaited(_refreshConversations(silent: true));
      unawaited(_loadChipCounts());
    });
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
}
