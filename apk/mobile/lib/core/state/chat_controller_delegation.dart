part of 'chat_controller.dart';

/// Delegation cards: the detail record and cancel. The cards live in the
/// reducer; this file holds the calls that update them.
extension ChatControllerDelegation on ChatController {
  /// The whole delegation record, including its result body and children.
  Future<({ChatDelegation? detail, ChatProblem? failure})> delegationDetail(
    String id,
  ) async {
    try {
      final body = await _execute(planChatDelegation(delegationId: id));
      final detail = parseChatDelegation(json: body);
      if (detail == null) {
        return (
          detail: null,
          failure: _localFailure('El equipo no devolvió la delegación.'),
        );
      }
      _reducer.updateCard(
        delegationId: detail.id,
        status: detail.state,
        summary: '',
      );
      _notify();
      return (detail: detail, failure: null);
    } catch (error) {
      return (detail: null, failure: _explain(error, WriteAction.read));
    }
  }

  /// Cancel keeps the card and its history; only the state moves to
  /// `cancelled`. SSE confirms the change, the local reducer hides the lag.
  Future<ChatProblem?> cancelDelegation(String id) async {
    try {
      final body = await _execute(planChatCancelDelegation(delegationId: id));
      final detail = parseChatDelegation(json: body);
      final status = detail?.state ?? 'cancelled';
      _reducer.updateCard(
        delegationId: id,
        status: status,
        summary: 'Cancelada por ti',
      );
      _lastSync = DateTime.now();
      _notify();
      unawaited(_refreshCurrent());
      return null;
    } catch (error) {
      return _explain(error, WriteAction.respond);
    }
  }
}
