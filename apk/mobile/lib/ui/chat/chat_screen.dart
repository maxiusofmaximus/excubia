import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/chat/attach_model.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';

import '../../core/api/alerts_api.dart';
import '../../core/api/chat_attachment_source.dart';
import '../../core/api/connection_failure.dart';
import '../../core/api/connection_text.dart';
import '../../core/state/chat_controller.dart';
import '../../core/state/hires_controller.dart';
import '../agent_detail_screen.dart';
import '../alerts/alerts_screen.dart';
import '../mail/mail_draft_screen.dart';
import '../mail/mail_settings_screen.dart';
import '../team/hires_section.dart';
import '../theme.dart';
import '../widgets/agent_avatar.dart';
import '../widgets/time_label.dart';
import 'approval_entry.dart';
import 'approval_inbox.dart';
import 'chat_attachments_picker.dart';
import 'chat_composer.dart';
import 'chat_progress.dart';
import 'chat_replies.dart';
import 'chat_starter_suggestions.dart';
import 'chat_conversation_view.dart';
import 'delegation_card.dart';
import 'delegation_detail_screen.dart';
import 'history_drawer.dart';

part 'chat_screen_actions.dart';
part 'chat_screen_chrome.dart';
part 'chat_screen_empty.dart';

/// How far from the end, in logical pixels, the timeline counts as away.
const _awayDistance = 140.0;

/// Alfred's messages newer than [seqAtBottom]: what «Ir al final» counts.
int unseenAgentMessages(List<ChatEntry> entries, int seqAtBottom) => entries
    .where(
      (entry) =>
          entry.kind == 'message' &&
          entry.authorKind == 'agent' &&
          entry.seq > seqAtBottom,
    )
    .length;

/// The chat-first home. The empty state is the product's front door; a
/// conversation is the same screen once it has a timeline.
class ChatScreen extends StatefulWidget {
  const ChatScreen({
    super.key,
    required this.controller,
    required this.hires,
    required this.onOpenTeam,
    required this.onUnpaired,
    this.pickImage = pickChatImage,
    this.pickDocument = pickChatDocument,
  });

  final ChatController controller;
  final HiresController hires;
  final VoidCallback onOpenTeam;
  final Future<void> Function() onUnpaired;

  /// Opens the gallery for one image; tests pass a fake.
  final Future<ChatAttachmentSource?> Function() pickImage;

  /// Opens the file manager for one document; tests pass a fake.
  final Future<ChatAttachmentSource?> Function() pickDocument;

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  final GlobalKey<ScaffoldState> _scaffold = GlobalKey<ScaffoldState>();
  final StepAnnouncer _steps = StepAnnouncer();
  final TextEditingController _composer = TextEditingController();
  final FocusNode _focus = FocusNode();
  final ScrollController _scroll = ScrollController();

  /// The conversation whose end is already shown; a new one opens at its end.
  String? _pinnedId;

  /// True while the timeline is more than [_awayDistance] from its end.
  final ValueNotifier<bool> _away = ValueNotifier<bool>(false);

  /// Alfred's messages that arrived while away; only «Ir al final» listens.
  final ValueNotifier<int> _unseen = ValueNotifier<int>(0);

  /// The newest seq seen while at the end; the unseen count starts after it.
  int _seqAtBottom = 0;

  /// One key per timeline row, by seq, so a quote can scroll to its message.
  final Map<int, GlobalKey> _rowKeys = <int, GlobalKey>{};

  ChatController get _chat => widget.controller;

  @override
  void initState() {
    super.initState();
    _chat.addListener(_onChanged);
    _scroll.addListener(_onScroll);
    _syncPin();
  }

  @override
  void dispose() {
    _chat.removeListener(_onChanged);
    // Leaving the screen drops the composer's files, so none is left on the host.
    unawaited(_chat.attachments.discard());
    _composer.dispose();
    _focus.dispose();
    _scroll.dispose();
    _away.dispose();
    _unseen.dispose();
    super.dispose();
  }

  void _onChanged() {
    if (!mounted) return;
    _syncPin();
    setState(() {});
    // The new row is laid out after this frame; only then is the distance known.
    WidgetsBinding.instance.addPostFrameCallback((_) => _checkAway());
  }

  /// Once a conversation has loaded, its timeline starts at the last message.
  void _syncPin() {
    final id = _chat.current?.id;
    if (id == null || id == _pinnedId || _chat.loadingConversation) return;
    // Files picked for another conversation cannot ride on this one's message.
    if (_chat.attachments.conversationId != id) {
      unawaited(_chat.attachments.discard());
    }
    _pinnedId = id;
    _away.value = false;
    _unseen.value = 0;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!_scroll.hasClients) return;
      _scroll.jumpTo(_scroll.position.maxScrollExtent);
    });
  }

  void _onScroll() {
    _checkAway();
    if (!_scroll.hasClients || _chat.loadingMore) return;
    if (_scroll.position.pixels <= 120 && _chat.hasMore) {
      unawaited(_chat.loadOlder());
    }
  }

  /// Away from the end, Alfred's new messages count; at the end the count
  /// restarts from the newest seq.
  void _checkAway() {
    if (!mounted || !_scroll.hasClients) return;
    final position = _scroll.position;
    final away = position.maxScrollExtent - position.pixels > _awayDistance;
    if (!away) _seqAtBottom = _chat.lastSeq;
    _away.value = away;
    _unseen.value = away ? unseenAgentMessages(_chat.entries, _seqAtBottom) : 0;
  }

  /// «Ir al final»: a jump, not an animation, so the button never reappears
  /// midway; the count restarts.
  void _jumpToEnd() {
    _seqAtBottom = _chat.lastSeq;
    _away.value = false;
    _unseen.value = 0;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!_scroll.hasClients) return;
      _scroll.jumpTo(_scroll.position.maxScrollExtent);
    });
  }

  void _jumpToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!_scroll.hasClients) return;
      _scroll.animateTo(
        _scroll.position.maxScrollExtent,
        duration: const Duration(milliseconds: 220),
        curve: Curves.easeOut,
      );
    });
  }

  /// The codes whose text goes back to the composer: the quote or a file is gone.
  static const _restoresText = {'unknown_reply_to', 'unknown_attachment'};

  Future<void> _send() async {
    final text = _composer.text.trim();
    final files = _chat.attachments.readyFiles;
    if ((text.isEmpty && files.isEmpty) ||
        !_chat.canSend ||
        !_chat.attachments.allReady) {
      return;
    }
    final replyTo = _chat.replyTo;
    _composer.clear();
    await _deliver(text, replyTo: replyTo, files: files);
    final failure = _chat.failure;
    if (failure == null || !_restoresText.contains(failure.code) || !mounted) {
      return;
    }
    // The quoted message or a file is gone: the text goes back so it can be sent again.
    _composer.text = text;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text('${failure.message} ${failure.action ?? ''}')),
    );
  }

  /// A chip sends its text as a normal message, with no reply attached.
  Future<void> _sendSuggestion(String text) => _deliver(text);

  Future<bool> _deliver(
    String text, {
    ChatEntry? replyTo,
    List<ChatAttachment> files = const [],
  }) async {
    _jumpToEnd();
    final sent = await _chat.send(text, replyTo: replyTo, files: files);
    _jumpToEnd();
    return sent;
  }

  /// Adds one picked file to the composer. A conversation is opened first:
  /// a file belongs to the one it was uploaded into.
  Future<void> _attach(ChatAttachKind kind) async {
    final source = switch (kind) {
      ChatAttachKind.image => await widget.pickImage(),
      ChatAttachKind.file => await widget.pickDocument(),
    };
    if (source == null || !mounted) return;
    final id = _chat.current?.id ?? await _chat.createConversation();
    if (id == null) return;
    await _chat.attachments.add(id, [source]);
  }

  Future<void> _newChat() async {
    final id = await _chat.createConversation();
    if (id != null) _focus.requestFocus();
  }

  void _suggest(String text) {
    _composer.text = text;
    _composer.selection = TextSelection.collapsed(offset: text.length);
    _focus.requestFocus();
    _jumpToBottom();
  }

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final step = _chat.turnProgress;
    final stepLine = step == null ? null : '${_chat.alfredName} · $step';
    return Scaffold(
      key: _scaffold,
      backgroundColor: c.bg,
      drawer: HistoryDrawer(
        controller: _chat,
        onOpenTeam: widget.onOpenTeam,
        onUnpaired: widget.onUnpaired,
      ),
      body: SafeArea(
        child: Column(
          children: [
            ChatHeader(
              name: _chat.alfredName,
              status: _chat.awaitingApproval
                  ? 'Esperando que aprueben este dispositivo'
                  : _chat.turnInFlight
                  ? stepLine ?? 'trabajando…'
                  : _chat.alfredStatus,
              announce: _steps.label(
                _chat.turnInFlight ? stepLine : null,
                DateTime.now(),
              ),
              offline: _chat.offline,
              connectionFailure: _chat.connectionFailure,
              unread: _chat.unreadMessages,
              onHistory: _openHistory,
              onNewChat: _newChat,
              thinking: _chat.turnInFlight,
              approvalsPending: _chat.approvals?.userPending().length ?? 0,
              onApprovals: _chat.approvals == null ? null : _openApprovals,
              alertRules: _chat.alertRules,
            ),
            if (_chat.approvals case final approvals?)
              ApprovalInbox(
                controller: approvals,
                projects: _chat.projects,
                alfred: _chat.alfredSeat,
              ),
            Expanded(
              child: _chat.loading && _chat.current == null
                  ? const Center(child: CircularProgressIndicator())
                  : _chat.current == null
                  ? _EmptyChat(
                      chat: _chat,
                      hires: widget.hires,
                      onSuggestion: _suggest,
                      onOpenTeam: widget.onOpenTeam,
                    )
                  : Stack(
                      children: [
                        ConversationView(
                          chat: _chat,
                          starters: StarterSuggestions(
                            chat: _chat,
                            hires: widget.hires,
                            onSuggestion: _suggest,
                          ),
                          scroll: _scroll,
                          rowKey: _rowKey,
                          onRetrySend: _chat.retryLocal,
                          onRetryTurn: _chat.retryTurn,
                          onRetryFailure: _chat.retryFailure,
                          onOpenDelegation: _openDelegation,
                          onOpenAgent: _openAgent,
                          onCancelDelegation: _cancelDelegation,
                          onReply: _chat.startReply,
                          onOpenQuote: _openQuote,
                          onSuggestion: _sendSuggestion,
                          onOpenDraft: _openDraft,
                        ),
                        Positioned(
                          right: Spacing.lg,
                          bottom: Spacing.md,
                          child: _JumpToEnd(
                            away: _away,
                            unseen: _unseen,
                            onTap: _jumpToEnd,
                          ),
                        ),
                      ],
                    ),
            ),
            if (_chat.replyTo case final target?)
              ReplyBar(
                target: target,
                agentName: _chat.alfredName,
                onCancel: _chat.cancelReply,
              ),
            ChatComposer(
              controller: _composer,
              focus: _focus,
              enabled: _chat.canSend,
              sending: _chat.sending,
              offline: _chat.offline,
              busy: _chat.turnInFlight,
              canStop: _chat.canStop,
              stopDenial: _chat.stopDenial,
              attachments: _chat.attachments,
              voice: _chat.voice,
              voiceSend: _chat.voiceSend,
              onVoiceSend: _deliver,
              onAttach: _attach,
              onSend: _send,
              onStop: _stop,
            ),
          ],
        ),
      ),
    );
  }
}
