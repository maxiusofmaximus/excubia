import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:excubia/src/rust/api/chat/attach_model.dart'
    show ChatAttachment;
import 'package:excubia/src/rust/api/chat/attach_plan.dart'
    show planAttachmentContent, planChatSendWithAttachments;
import 'package:excubia/src/rust/api/chat/model.dart';
import 'package:excubia/src/rust/api/chat/model_team.dart';
import 'package:excubia/src/rust/api/chat/plan.dart';
import 'package:excubia/src/rust/api/chat/reducer.dart';
import 'package:excubia/src/rust/api/error/chat.dart';
import 'package:excubia/src/rust/api/error.dart'
    show FailureKind, WriteAction;
import 'package:excubia/src/rust/api/state.dart' show AgentEntry;
import 'package:excubia/src/rust/api/write.dart' show HttpCall;

import '../api/chat_document_files.dart';
import '../api/chat_document_previews.dart';
import '../api/chat_transport.dart';
import '../api/connection_failure.dart';
import '../api/connection_text.dart';
import '../api/host_client.dart';
import '../api/agenda_api.dart';
import '../api/restore_api.dart';
import '../api/alerts_api.dart';
import '../api/alfred_backend_api.dart';
import '../api/mail_api.dart';
import '../api/plan_runner.dart';
import '../model/connection_state.dart';
import 'approvals_controller.dart';
import 'chat_attachment_errors.dart';
import 'chat_attachment_images.dart';
import 'chat_attachment_tray.dart';
import 'chat_document_opener.dart';
import 'live_triage.dart';
import 'mail_drafts.dart';
import 'voice_notes.dart';
import 'voice_recorder.dart';
import 'voice_send_settings.dart';

part 'chat_controller_attachments.dart';
part 'chat_controller_delegation.dart';
part 'chat_controller_history.dart';
part 'chat_controller_send.dart';
part 'chat_controller_stop.dart';
part 'chat_controller_sync.dart';

/// Which slice of the history the drawer asks the host for, in chip order.
enum HistoryFilter { all, attention, working, pinned, sections, archived }

/// The newest message the operator wrote, sent or not; null when there is none.
ChatEntry? latestOwnMessage(List<ChatEntry> entries) {
  for (final entry in entries.reversed) {
    if (entry.kind == 'message' && entry.authorKind == 'user') return entry;
  }
  return null;
}

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
    VoiceSendSettings? voiceSend,
    this.searchDelay = const Duration(milliseconds: 220),
    this.refreshDelay = const Duration(milliseconds: 180),
  }) : voiceSend = voiceSend ?? VoiceSendSettings(),
       _reducer = newChatReducer();

  final ChatTransport _transport;
  final LiveTriage _triage;

  /// Approval state, fed from the same SSE stream; null in tests without it.
  final ApprovalsController? approvals;

  /// The composer's voice-note destination («Al terminar de hablar»).
  final VoiceSendSettings voiceSend;
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
  ChatCounts? _counts;
  bool _countsSupported = true;
  final Map<String, int> _readSent = <String, int>{};
  ChatEntry? _replyTo;
  bool _stopping = false;
  ChatProblem? _stopDenial;

  /// The reply each unsent `client_id` answers, so a retry keeps its quote.
  final Map<String, ChatEntry> _replyByClient = <String, ChatEntry>{};

  /// The files each unsent `client_id` names, so a retry keeps them too.
  final Map<String, List<ChatAttachment>> _attachmentsByClient =
      <String, List<ChatAttachment>>{};

  /// The composer's files; they are uploaded here and named by the next send.
  late final ChatAttachmentTray attachments = ChatAttachmentTray(_transport);

  /// The stored images of the timeline, by attachment id.
  late final ChatAttachmentImages images = ChatAttachmentImages(
    (id) => _attachmentBytes(id),
  );

  /// The documents of the timeline: opened with the system, saved, or previewed.
  late final ChatDocumentOpener documents = ChatDocumentOpener(
    fetch: (id) => _attachmentBytes(id),
    files: const SystemChatDocumentFiles(),
    root: chatDocumentsDirectory,
    previews: ChatDocumentPreviews(_transport, conversation: () => current?.id),
  );

  ChatBootstrap? get bootstrap => _bootstrap;
  List<ChatProject> get projects =>
      _bootstrap?.projects ?? const <ChatProject>[];
  LiveTriage get triage => _triage;

  /// The mail routes over the same signed transport as the chat.
  MailApi get mail => MailApi(_transport);

  /// The agenda routes (notes and contacts) over the same signed transport.
  AgendaApi get agenda => AgendaApi(_transport);

  /// The copies of the turn («Copias del turno») over the same signed transport.
  RestoreApi get restore => RestoreApi(_transport);

  /// The alert rules and the alerts mode, over the same signed transport.
  AlertsApi get alertRules => AlertsApi(_transport);

  /// The engine of Alfred (Claude or the local model), over the same signed transport.
  AlfredBackendApi get alfredBackend => AlfredBackendApi(_transport);

  /// The draft sheets opened from the timeline. A send under way keeps its
  /// watch here, so it ends even when its sheet is closed.
  late final MailDrafts mailDrafts = MailDrafts(MailApi(_transport));

  /// The composer's voice notes: recorded on the phone, transcribed on the host.
  late final VoiceNotes voice = VoiceNotes(_transport, RecordVoiceRecorder());
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

  /// The message the next send answers; null when the operator is not replying.
  ChatEntry? get replyTo => _replyTo;

  void startReply(ChatEntry entry) {
    _replyTo = entry;
    _notify();
  }

  void cancelReply() {
    if (_replyTo == null) return;
    _replyTo = null;
    _notify();
  }

  /// A turn of the open conversation is queued or running. The newest own
  /// message's turn is the fact; the list's count of open runs only stands in
  /// until that message has a turn state, because the list can be older.
  bool get turnInFlight {
    final state = latestOwnMessage(entries)?.turnState;
    if (state != null) return state == 'queued' || state == 'running';
    return (_current?.activeRuns ?? 0) > 0;
  }

  /// The step Alfred set on a conversation's open turn, as one plain line.
  String? progressOf(String conversationId) =>
      _reducer.progressOf(conversationId: conversationId);

  /// The step of the open conversation, while its turn is in flight.
  String? get turnProgress {
    final id = _current?.id;
    if (id == null || !turnInFlight) return null;
    return progressOf(id);
  }

  /// The stop button works while a turn is in flight, once per tap, and not
  /// after the host refused this device the `respond` grant.
  bool get canStop => turnInFlight && !_stopping && _stopDenial == null;

  /// Why the last stop was refused for want of the grant; it keeps the button
  /// off until the next turn is sent.
  ChatProblem? get stopDenial => _stopDenial;

  DateTime? get lastSync => _lastSync;
  int get pinnedCount => _pinnedCount;
  int get archivedCount => _archivedCount;

  /// The chip numbers; null until the host answers. The new chips show only
  /// while [countsSupported] is true.
  ChatCounts? get counts => _counts;
  bool get countsSupported => _countsSupported;
  String get alfredName => _bootstrap?.alfredName ?? 'Alfred';

  /// Alfred as a seat, so an approval he asks for reads as his name; null until
  /// the bootstrap arrives or when the host sends no agent id.
  ChatSeat? get alfredSeat {
    final boot = _bootstrap;
    final id = boot?.alfredAgentId;
    if (boot == null || id == null) return null;
    return ChatSeat(
      agentId: id,
      name: boot.alfredName,
      status: boot.alfredStatus,
    );
  }

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

  ConnectionFailure? get connectionFailure => _triage.connectionFailure;

  /// The device is paired but no one has approved its read_state yet.
  bool get awaitingApproval => _triage.connection is AwaitingApproval;

  bool get canSend => !offline && !awaitingApproval && !_sending;

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

  Stream<ChatProblem> get failures => _failures.stream;

  void clearFailure() {
    if (_failure == null) return;
    _failure = null;
    _notify();
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
    if (error is HostError) {
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
    mailDrafts.dispose();
    attachments.dispose();
    voice.dispose();
    super.dispose();
  }
}

class _LocalChatProblem implements Exception {
  const _LocalChatProblem(this.failure);

  final ChatProblem failure;
}
