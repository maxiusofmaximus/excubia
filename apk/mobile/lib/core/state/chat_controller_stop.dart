part of 'chat_controller.dart';

/// The stop button: ends the open turn of the current conversation. The turn's
/// new state arrives over SSE, so a success changes nothing here.
extension ChatControllerStop on ChatController {
  /// The refusal to show, or null when the stop went through or no turn was
  /// open. A second tap while the first request flies does nothing.
  Future<ChatProblem?> stop() async {
    final id = _current?.id;
    if (id == null || !canStop) return null;
    _stopping = true;
    _notify();
    try {
      await _execute(planChatStop(conversationId: id));
      return null;
    } catch (error) {
      final failure = _explainStop(error);
      if (failure.code == 'forbidden') _stopDenial = failure;
      return failure;
    } finally {
      _stopping = false;
      _notify();
    }
  }

  ChatProblem _explainStop(Object error) {
    if (error is HostError) {
      return explainChatStopError(
        status: error.status,
        body: error.body,
        retryAfter: error.retryAfter,
      );
    }
    return _explain(error, WriteAction.respond);
  }
}
