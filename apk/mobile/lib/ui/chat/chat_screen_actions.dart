part of 'chat_screen.dart';

/// Actions that scroll to a quote or open another screen. Each reads the state
/// of [_ChatScreenState]; the state keeps the build and the send path.
extension _ChatScreenActions on _ChatScreenState {
  /// Scrolls to the quoted message when its row is built; one not loaded yet
  /// stays where it is.
  void _openQuote(String messageId) {
    for (final entry in _chat.entries) {
      if (entry.messageId != messageId) continue;
      final row = _rowKeys[entry.seq]?.currentContext;
      if (row != null) {
        unawaited(
          Scrollable.ensureVisible(
            row,
            duration: const Duration(milliseconds: 250),
            alignment: 0.5,
          ),
        );
      }
      return;
    }
  }

  Key _rowKey(int seq) => _rowKeys.putIfAbsent(seq, () => GlobalKey());

  Future<void> _openDelegation(ChatEntry entry) async {
    final id = entry.delegationId;
    if (id == null || id.isEmpty) return;
    await Navigator.of(context).push<void>(
      MaterialPageRoute<void>(
        builder: (_) => DelegationDetailScreen(
          chat: _chat,
          delegationId: id,
          fallbackTitle: entry.title ?? 'Delegación',
        ),
      ),
    );
  }

  Future<void> _openAgent(ChatEntry entry) async {
    final paneId = entry.paneId;
    if (paneId == null || paneId.isEmpty) return;
    await Navigator.of(context).push<void>(
      MaterialPageRoute<void>(
        builder: (_) => AgentDetailScreen(triage: _chat.triage, paneId: paneId),
      ),
    );
  }

  Future<void> _cancelDelegation(ChatEntry entry) async {
    final id = entry.delegationId;
    if (id == null || id.isEmpty) return;
    final confirmed = await confirmDelegationCancel(
      context,
      title: entry.title ?? 'La delegación',
    );
    if (!confirmed || !mounted) return;
    final failure = await _chat.cancelDelegation(id);
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(
          failure == null
              ? 'Delegación cancelada. El historial se conserva.'
              : failure.message,
        ),
      ),
    );
  }

  /// The refusal is said once in a SnackBar; a device without the grant keeps
  /// the button off with the same text in the composer.
  Future<void> _stop() async {
    final failure = await _chat.stop();
    if (failure == null || !mounted) return;
    ScaffoldMessenger.of(context)
        .showSnackBar(SnackBar(content: Text(failure.message)));
  }

  Future<void> _openHistory() async {
    await _chat.loadHistory();
    if (!mounted) return;
    _scaffold.currentState?.openDrawer();
  }

  void _openApprovals() {
    final approvals = _chat.approvals;
    if (approvals == null) return;
    openApprovalSettings(
      context,
      approvals: approvals,
      conversationId: _chat.current?.id,
      projects: _chat.projects,
      alfred: _chat.alfredSeat,
    );
  }

  /// Opens «Revisar borrador». «Pedir cambios a Alfred» names the draft in the
  /// composer, so Alfred has its id.
  Future<void> _openDraft(ChatEntry entry) async {
    final draft = entry.mailDraft;
    if (draft == null) return;
    final choice = await showMailDraftSheet(
      context,
      controller: _chat.mailDrafts.draftFor(draft.account, draft.draftId),
      approvals: _chat.approvals,
      alfredCanRevise: true,
      agentName: _chat.alfredName,
      onOpenSettings: () => Navigator.of(context).push(
        MaterialPageRoute<void>(
          builder: (_) => MailSettingsScreen(mail: _chat.mail),
        ),
      ),
    );
    if (choice == MailDraftChoice.requestChanges && mounted) {
      _suggest('Cambia el borrador ${draft.draftId}: ');
    }
  }
}
