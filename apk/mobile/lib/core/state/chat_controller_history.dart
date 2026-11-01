part of 'chat_controller.dart';

/// The conversation list behind the drawer: search, filters, history pages,
/// sections and the pinned and archived counts. The state stays in the
/// controller; this file groups the code that reads and writes it.
extension ChatControllerHistory on ChatController {
  void setQuery(String value) {
    if (value == _query) return;
    _query = value;
    _notify();
    _searchTimer?.cancel();
    _searchTimer = Timer(searchDelay, () {
      unawaited(_refreshConversations());
    });
  }

  void setFilter(HistoryFilter value) {
    if (value == _filter) return;
    _filter = value;
    _notify();
    unawaited(_refreshConversations());
  }

  Future<void> loadHistory() async {
    await Future.wait([_loadSections(), _refreshConversations()]);
    await _loadCounts();
    _notify();
  }

  Future<ChatProblem?> rename(String id, String title) =>
      _action(planChatRenameConversation(conversationId: id, title: title));

  Future<ChatProblem?> togglePin(ChatConversation conversation) => _action(
    planChatPinConversation(
      conversationId: conversation.id,
      pinned: !conversation.pinned,
    ),
  );

  Future<ChatProblem?> moveToSection(String id, String? sectionId) => _action(
    planChatMoveConversation(conversationId: id, sectionId: sectionId),
  );

  Future<ChatProblem?> archive(ChatConversation conversation) async {
    final failure = await _action(
      planChatArchiveConversation(
        conversationId: conversation.id,
        archived: !conversation.archived,
      ),
    );
    if (failure == null &&
        !conversation.archived &&
        _current?.id == conversation.id) {
      _current = null;
      _reducer.reset();
      _notify();
    }
    return failure;
  }

  /// Deletes for good. A host that no longer has the conversation (404) counts
  /// as deleted; a busy one (409) keeps it and its refusal says what to stop.
  Future<ChatProblem?> deleteConversation(ChatConversation conversation) async {
    try {
      await _execute(
        planChatDeleteConversation(conversationId: conversation.id),
      );
    } catch (error) {
      final gone = error is GatewayError && error.status == 404;
      if (!gone) {
        final failure = _explain(error, WriteAction.respond);
        _failures.add(failure);
        return failure;
      }
    }
    _forgetConversation(conversation.id);
    await _loadCounts();
    return null;
  }

  /// Takes a conversation out of the list and, when it is open, out of the
  /// timeline. The counts are read again by the caller.
  void _forgetConversation(String id) {
    _conversations = _conversations.where((chat) => chat.id != id).toList();
    if (_current?.id == id) {
      _current = null;
      _reducer.reset();
    }
    _notify();
  }

  Future<ChatSection?> createSection(String name) async {
    final plan = planChatSection(name: name);
    try {
      final body = await _execute(plan);
      final section = parseChatSection(json: body);
      if (section != null) {
        _sections = [..._sections, section]
          ..sort((a, b) => a.order.compareTo(b.order));
        _notify();
      }
      return section;
    } catch (error) {
      _failure = _explain(error, WriteAction.respond);
      _notify();
      return null;
    }
  }

  Future<void> _loadSections() async {
    try {
      final body = await _execute(planChatSections());
      final sections = parseChatSections(json: body);
      if (sections != null) _sections = sections;
    } catch (_) {
      // Sections are optional decoration for the history; keep the last list.
    }
  }

  Future<void> _refreshConversations({bool silent = false}) async {
    final archived = _filter == HistoryFilter.archived;
    final pinned = _filter == HistoryFilter.pinned ? true : null;
    final plan = planChatConversations(
      query: _query,
      pinned: pinned,
      archived: archived,
      attention: _filter == HistoryFilter.attention,
      working: _filter == HistoryFilter.working,
    );
    try {
      final body = await _execute(plan);
      final page = parseChatConversations(json: body);
      if (page == null) return;
      _conversations = page.conversations;
      _reducer.acceptConversations(conversations: page.conversations);
      if (_current != null) {
        final updated = _conversationFor(_current!.id);
        if (updated != null) _current = updated;
      }
      // The list shows no open run but our newest message still reads as
      // in flight: its frame was lost, so the timeline is read once more.
      if (_current?.activeRuns == 0 && turnInFlight) {
        unawaited(_refreshCurrent());
      }
      _lastSync = DateTime.now();
      if (!silent) _notify();
    } catch (_) {
      if (!silent) _notify();
    }
  }

  Future<void> _loadCounts() async {
    try {
      final counts = await Future.wait([
        _countAll(pinned: true),
        _countAll(archived: true),
      ]);
      _pinnedCount = counts[0];
      _archivedCount = counts[1];
    } catch (_) {
      // Counts are decoration; keep the last known values.
    }
    await _loadChipCounts();
  }

  /// The chip numbers in one request, also after each chat event. A host
  /// without the route hides the new chips and drops a filter it cannot answer.
  Future<void> _loadChipCounts() async {
    try {
      final counts = parseChatCounts(json: await _execute(planChatCounts()));
      if (counts == null) return;
      _counts = counts;
      _countsSupported = true;
      _notify();
    } catch (error) {
      if (error is! GatewayError ||
          !chatCountsUnsupported(status: error.status)) {
        return;
      }
      _countsSupported = false;
      _counts = null;
      if (_filter == HistoryFilter.attention ||
          _filter == HistoryFilter.working) {
        _filter = HistoryFilter.all;
        unawaited(_refreshConversations());
      }
      _notify();
    }
  }

  /// Every conversation that matches, following `next_cursor` page by page. The
  /// host has no total, so a count from one page would stop at the page size.
  Future<int> _countAll({bool? pinned, bool? archived}) async {
    var total = 0;
    String? cursor;
    do {
      final page = parseChatConversations(
        json: await _execute(
          planChatConversations(
            pinned: pinned,
            archived: archived,
            attention: false,
            working: false,
            cursor: cursor,
          ),
        ),
      );
      if (page == null) break;
      total += page.conversations.length;
      final next = page.nextCursor;
      cursor = next == cursor ? null : next;
    } while (cursor != null);
    return total;
  }
}
