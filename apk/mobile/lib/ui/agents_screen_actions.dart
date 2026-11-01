part of 'agents_screen.dart';

extension _AgentsScreenActions on _AgentsScreenState {
  Future<void> _openNewConversation({String? cwd}) async {
    final created = await Navigator.of(context).push<CreatedWorkspace>(
      MaterialPageRoute<CreatedWorkspace>(
        builder: (_) => NewConversationScreen(triage: _triage, initialCwd: cwd),
      ),
    );
    if (created == null || !mounted) return;
    _awaitPane(created.paneId);
  }

  void _awaitPane(String paneId) {
    _pendingPane = paneId;
    _pendingTimer?.cancel();
    _pendingTimer = Timer(const Duration(seconds: 8), () {
      if (!mounted || _pendingPane == null) return;
      _pendingPane = null;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text(
            'La conversación se creó; aparecerá en la lista al conectarse.',
          ),
        ),
      );
    });
    unawaited(_openPendingIfPresent());
  }

  Future<void> _openPendingIfPresent() async {
    final paneId = _pendingPane;
    if (paneId == null) return;
    final state = _triage.currentState;
    if (state == null || !state.agents.any((agent) => agent.paneId == paneId)) {
      return;
    }
    _pendingPane = null;
    _pendingTimer?.cancel();
    await _openDetail(paneId);
  }

  Future<void> _openDetail(String paneId) async {
    final result = await Navigator.of(context).push<AgentDetailResult>(
      MaterialPageRoute<AgentDetailResult>(
        builder: (_) => AgentDetailScreen(triage: _triage, paneId: paneId),
      ),
    );
    if (!mounted || result == null) return;
    switch (result) {
      case AgentStopped(:final alreadyGone):
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              alreadyGone
                  ? 'El agente ya había terminado.'
                  : 'Agente detenido.',
            ),
          ),
        );
      case ConversationCreated(:final created):
        _awaitPane(created.paneId);
    }
  }
}
