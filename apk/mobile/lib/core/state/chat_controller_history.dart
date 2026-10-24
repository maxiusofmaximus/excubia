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
    );
    try {
      final body = await _execute(plan);
      final page = parseChatConversations(json: body);
      if (page == null) return;
      _conversations = page.conversations;
      if (_current != null) {
        final updated = _conversationFor(_current!.id);
        if (updated != null) _current = updated;
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
