import 'dart:async';

import 'package:flutter/foundation.dart';

import '../../src/rust/api/chat/attach_model.dart';
import '../../src/rust/api/chat/attach_plan.dart';
import '../api/chat_attachment_source.dart';
import '../api/chat_transport.dart';
import '../api/plan_runner.dart';
import 'chat_attachment_errors.dart';
import 'chat_attachment_upload.dart';

enum DraftStatus { queued, uploading, ready, failed }

/// One image in the composer's tray, from pick to ready. Only the tray changes
/// it.
class AttachmentDraft {
  AttachmentDraft(this.conversationId, this.source);

  final String conversationId;
  final ChatAttachmentSource source;
  DraftStatus status = DraftStatus.queued;
  double progress = 0;

  /// The host's id, once the upload exists there; `DELETE` needs it.
  String? remoteId;

  /// The host's file, once it is ready; a message names its id.
  ChatAttachment? file;

  /// What the user reads when [status] is failed.
  String? error;
  bool cancelled = false;

  String get name => source.name;

  void fail(String text) {
    status = DraftStatus.failed;
    error = text;
  }
}

/// The files the composer is holding: up to ten, uploaded one after another.
/// A message may be sent only when every file is ready.
class ChatAttachmentTray extends ChangeNotifier {
  ChatAttachmentTray(this._transport, {Future<void> Function(Duration)? pause})
    : _pause = pause ?? Future<void>.delayed;

  final ChatTransport _transport;
  final Future<void> Function(Duration) _pause;
  final List<AttachmentDraft> _drafts = <AttachmentDraft>[];
  Future<void> _queue = Future<void>.value();
  bool _disposed = false;

  List<AttachmentDraft> get items => List.unmodifiable(_drafts);
  bool get canAdd => _drafts.length < maxChatAttachments;

  /// True when no file is still uploading or refused; an empty tray is ready.
  bool get allReady => _drafts.every((d) => d.status == DraftStatus.ready);

  /// The conversation the held files belong to; null when the tray is empty.
  String? get conversationId =>
      _drafts.isEmpty ? null : _drafts.first.conversationId;

  /// The files a message can name, in tray order.
  List<ChatAttachment> get readyFiles => [
    for (final draft in _drafts) ?draft.file,
  ];

  /// Adds picked images up to the limit. A file that fails the checks is kept
  /// as a failed draft, so the user sees why and removes it.
  Future<void> add(
    String conversationId,
    List<ChatAttachmentSource> sources,
  ) async {
    if (_drafts.any((d) => d.conversationId != conversationId)) {
      await discard();
    }
    for (final source in sources) {
      if (!canAdd) break;
      final draft = AttachmentDraft(conversationId, source);
      _drafts.add(draft);
      final refusal = await _admit(source);
      if (refusal == null) {
        _queue = _queue.then((_) => _run(draft));
      } else {
        draft.fail(attachmentErrorText(refusal));
      }
      _notify();
    }
  }

  /// Takes [draft] out of the tray. An upload already on the host is dropped too.
  Future<void> remove(AttachmentDraft draft) async {
    _drafts.remove(draft);
    draft.cancelled = true;
    _notify();
    await _delete(draft);
  }

  /// Drops every draft, as when the screen closes or the conversation changes.
  Future<void> discard() async {
    if (_drafts.isEmpty) return;
    final gone = List.of(_drafts);
    _drafts.clear();
    for (final draft in gone) {
      draft.cancelled = true;
    }
    _notify();
    await Future.wait(gone.map(_delete));
  }

  /// Forgets the files a sent message took; the host now keeps them.
  void forgetSent(List<ChatAttachment> sent) {
    if (sent.isEmpty) return;
    _drafts.removeWhere((d) => sent.contains(d.file));
    _notify();
  }

  Future<String?> _admit(ChatAttachmentSource source) async {
    try {
      return await refusalOf(source);
    } catch (_) {
      return 'unreadable';
    }
  }

  Future<void> _run(AttachmentDraft draft) async {
    if (draft.cancelled) return;
    draft.status = DraftStatus.uploading;
    _notify();
    try {
      final file = await uploadAttachment(
        transport: _transport,
        conversationId: draft.conversationId,
        source: draft.source,
        isCancelled: () => draft.cancelled,
        onCreated: (id) => _adopt(draft, id),
        onProgress: (progress) {
          draft.progress = progress;
          _notify();
        },
        pause: _pause,
      );
      if (file != null) {
        draft.file = file;
        draft.status = DraftStatus.ready;
        draft.progress = 1;
      }
    } on AttachmentRefused catch (refusal) {
      draft.fail(attachmentErrorText(refusal.code));
    } catch (_) {
      draft.fail(attachmentErrorText('unreadable'));
    }
    _notify();
  }

  /// A draft removed while its create was in flight would leave the host copy
  /// behind, so it is dropped here as soon as its id arrives.
  void _adopt(AttachmentDraft draft, String remoteId) {
    draft.remoteId = remoteId;
    if (draft.cancelled) unawaited(_delete(draft));
  }

  /// Best effort: the draft is already off the screen, and a refused `DELETE`
  /// has no one to show it to.
  Future<void> _delete(AttachmentDraft draft) async {
    final remoteId = draft.remoteId;
    if (remoteId == null) return;
    draft.remoteId = null;
    try {
      await runPlan(
        _transport,
        planAttachmentDelete(
          conversationId: draft.conversationId,
          attachmentId: remoteId,
        ),
      );
    } catch (_) {
      // See above.
    }
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    unawaited(discard());
    super.dispose();
  }
}
