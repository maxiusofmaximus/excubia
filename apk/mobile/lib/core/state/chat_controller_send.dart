part of 'chat_controller.dart';

/// Sending: a new conversation on the first message, the turn itself and the
/// retries of a failed send. The send state stays in the controller; this file
/// groups the code that drives it.
extension ChatControllerSend on ChatController {
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
      _replyTo = null;
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
  /// the empty start screen needs no separate form. A reply names the message
  /// it answers; a chip sends without one.
  /// A message may be only files: the text can be empty when [files] names one.
  /// Returns whether the host took the message; false leaves the text with the caller.
  Future<bool> send(
    String raw, {
    ChatEntry? replyTo,
    List<ChatAttachment> files = const [],
  }) async {
    final text = raw.trim();
    if (text.isEmpty && files.isEmpty) return false;
    if (offline) {
      final failure = connectionFailure;
      _failure = offlineChatFailure(
        message: failure == null
            ? 'Sin conexión con el equipo.'
            : connectionText(failure),
        action: 'Reconecta para escribir a Alfred.',
      );
      _notify();
      return false;
    }
    var id = _current?.id;
    if (id == null) {
      id = await createConversation(
        title: text.isEmpty ? null : _titleFrom(text),
      );
      if (id == null) return false;
    }
    attachments.forgetSent(files);
    return _sendText(
      id,
      text,
      newChatClientId(),
      replyTo: replyTo,
      files: files,
    );
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
    final reply = _replyByClient.remove(old);
    final files = _attachmentsByClient.remove(old) ?? const <ChatAttachment>[];
    _reducer.discardSend(clientId: old);
    _failure = null;
    await _sendText(id, entry.body, clientId, replyTo: reply, files: files);
  }

  /// Retry a failed turn: send the user message that preceded the empty agent
  /// failure card again, with a new `client_id` (the failed one is durable).
  /// A message that carried files cannot be sent again: its files were used up.
  Future<void> retryTurn(ChatEntry failed) async {
    final id = _current?.id;
    if (id == null) return;
    final all = entries;
    final index = all.indexWhere((entry) => entry.seq == failed.seq);
    for (var cursor = index - 1; cursor >= 0; cursor--) {
      final candidate = all[cursor];
      if (candidate.authorKind == 'user' && candidate.body.isNotEmpty) {
        if (candidate.attachments?.isNotEmpty ?? false) {
          _failure = attachmentChatProblem('unknown_attachment');
          _notify();
          return;
        }
        await _sendText(id, candidate.body, newChatClientId());
        return;
      }
    }
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

  /// True when the host has the message; false means it never reached it.
  Future<bool> _sendText(
    String conversationId,
    String text,
    String clientId, {
    ChatEntry? replyTo,
    List<ChatAttachment> files = const [],
  }) async {
    final replyToId = replyTo?.messageId;
    final plan = files.isEmpty
        ? planChatSend(
            conversationId: conversationId,
            clientId: clientId,
            text: text,
            replyToId: replyToId,
          )
        : planChatSendWithAttachments(
            conversationId: conversationId,
            clientId: clientId,
            text: text,
            replyToId: replyToId,
            attachments: [for (final file in files) file.id],
          );
    if (replyTo != null) _replyByClient[clientId] = replyTo;
    if (files.isNotEmpty) _attachmentsByClient[clientId] = files;
    _reducer.beginSend(clientId: clientId, text: text);
    _sending = true;
    _failure = null;
    _stopDenial = null;
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
      _replyByClient.remove(clientId);
      _attachmentsByClient.remove(clientId);
      if (identical(_replyTo, replyTo)) _replyTo = null;
      _lastSync = DateTime.now();
      _markRead();
      return true;
    } catch (error) {
      final refused = namesUnknownAttachment(error);
      final failure = refused
          ? attachmentChatProblem('unknown_attachment')
          : _explain(error, WriteAction.respond);
      _failure = failure;
      if (failure.code == 'unknown_reply_to') {
        // The quoted message is gone: the text goes back to the composer, not
        // to a failed bubble, and the quote is dropped.
        _reducer.discardSend(clientId: clientId);
        _replyByClient.remove(clientId);
        if (identical(_replyTo, replyTo)) _replyTo = null;
      } else if (refused) {
        // A file is gone or was used: the text goes back to the composer and
        // the files are dropped, so the user attaches them again.
        _reducer.discardSend(clientId: clientId);
        _replyByClient.remove(clientId);
        _attachmentsByClient.remove(clientId);
        if (identical(_replyTo, replyTo)) _replyTo = null;
      } else if (accepted || failure.keptByHost) {
        // The host kept the turn (or accepted it but failed the run); the
        // timeline is the truth, so the optimistic bubble is not duplicated.
        _reducer.discardSend(clientId: clientId);
        unawaited(_refreshCurrent());
      } else {
        _reducer.failSend(clientId: clientId);
      }
      _retryClientId = error is HostError && error.status == 0
          ? clientId
          : null;
      return accepted || failure.keptByHost;
    } finally {
      _sending = false;
      _notify();
      unawaited(_refreshConversations(silent: true));
    }
  }

  String _titleFrom(String text) {
    final line = text.split('\n').first.trim();
    return line.length <= 60 ? line : '${line.substring(0, 59)}…';
  }
}
