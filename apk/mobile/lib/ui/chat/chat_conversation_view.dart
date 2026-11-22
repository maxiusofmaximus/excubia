import 'package:flutter/material.dart';
import 'package:excubia/src/rust/api/approvals/model.dart';
import 'package:excubia/src/rust/api/chat/model.dart';
import 'package:excubia/src/rust/api/error/chat.dart' show offlineChatFailure;

import '../../core/api/connection_text.dart';
import '../../core/state/chat_controller.dart';
import '../theme.dart';
import '../widgets/agent_portrait_card.dart';
import '../widgets/time_label.dart';
import 'approval_card.dart';
import 'chat_bubbles.dart';

/// The welcome of a new conversation: Alfred's portrait, a line, then the starter
/// chips. The portrait steps aside while the keyboard is up, so the field and the
/// chips stay in view, and it is smaller on a short screen.
class _Welcome extends StatefulWidget {
  const _Welcome({required this.alfredName, required this.starters});

  final String alfredName;
  final Widget starters;

  @override
  State<_Welcome> createState() => _WelcomeState();
}

/// View.of() does not rebuild when the keyboard opens or closes, so without this
/// the portrait stayed hidden after the keyboard a new chat opens on its own went away.
class _WelcomeState extends State<_Welcome> with WidgetsBindingObserver {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  void didChangeMetrics() => setState(() {});

  String get alfredName => widget.alfredName;
  Widget get starters => widget.starters;

  @override
  Widget build(BuildContext context) {
    // The chat's Scaffold strips the keyboard from MediaQuery, so the view's own inset is read.
    final keyboardUp = View.of(context).viewInsets.bottom > 0;
    final height = MediaQuery.sizeOf(context).height;
    // The card shrinks on short screens, so the portrait, the line and the chips fit one view.
    final cardWidth = height < 700 ? 110.0 : (height < 760 ? 130.0 : 150.0);
    // Tighter top and gaps: the third chip must show whole on a 412x915 phone, without scroll.
    return Padding(
      padding: const EdgeInsets.fromLTRB(Spacing.xl, Spacing.md, Spacing.xl, 0),
      child: Column(
        children: [
          if (!keyboardUp) ...[
            SizedBox(
              width: cardWidth,
              child: AgentPortraitCard(
                id: 'alfred',
                name: alfredName,
                centered: true,
              ),
            ),
            const SizedBox(height: Spacing.sm),
          ],
          Text(
            '¿Qué movemos hoy?',
            style: context.text.headline,
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: Spacing.md),
          starters,
        ],
      ),
    );
  }
}

/// Room under the welcome's last chip: the composer's one-line height plus 16 dp,
/// so the chip scrolls clear of it.
/// shortcut: fixed estimate; a reply bar or attachment tray is taller, upgrade by measuring the composer.
const double _welcomeBottomInset = 96;

/// One row per draft: the newest «Revisar borrador» card replaces the older ones
/// of the same draft; every other row stays as it came.
List<ChatEntry> visibleEntries(List<ChatEntry> entries) {
  final newest = <String, int>{};
  for (final entry in entries) {
    if (entry.mailDraft case final draft?) {
      newest.update(
        draft.draftId,
        (seq) => seq > entry.seq ? seq : entry.seq,
        ifAbsent: () => entry.seq,
      );
    }
  }
  return [
    for (final entry in entries)
      if (entry.mailDraft == null ||
          newest[entry.mailDraft!.draftId] == entry.seq)
        entry,
  ];
}

/// The newest agent message, while the operator has not written after it; only
/// it offers reply chips. Local bubbles come last, so an unsent one counts too.
ChatEntry? unansweredAgentMessage(List<ChatEntry> entries) {
  ChatEntry? agent;
  for (final entry in entries) {
    if (entry.kind != 'message') continue;
    if (entry.authorKind == 'agent') agent = entry;
    if (entry.authorKind == 'user') agent = null;
  }
  return agent;
}

class ConversationView extends StatelessWidget {
  const ConversationView({
    super.key,
    required this.chat,
    required this.starters,
    required this.scroll,
    required this.rowKey,
    required this.onRetrySend,
    required this.onRetryTurn,
    required this.onRetryFailure,
    required this.onOpenDelegation,
    required this.onOpenAgent,
    required this.onCancelDelegation,
    required this.onReply,
    required this.onOpenQuote,
    required this.onSuggestion,
    this.onOpenDraft,
  });

  final ChatController chat;

  /// The starter tiles shown in a new conversation with no message yet.
  final Widget starters;
  final ScrollController scroll;

  /// The key of the row for a seq, so a quote can find the row it names.
  final Key Function(int seq) rowKey;
  final ValueChanged<ChatEntry> onRetrySend;
  final ValueChanged<ChatEntry> onRetryTurn;
  final VoidCallback onRetryFailure;
  final ValueChanged<ChatEntry> onOpenDelegation;
  final ValueChanged<ChatEntry> onOpenAgent;
  final ValueChanged<ChatEntry> onCancelDelegation;
  final ValueChanged<ChatEntry> onReply;
  final ValueChanged<String> onOpenQuote;
  final ValueChanged<String> onSuggestion;
  final ValueChanged<ChatEntry>? onOpenDraft;

  @override
  Widget build(BuildContext context) {
    final entries = visibleEntries(chat.entries);
    final latest = latestOwnMessage(entries);
    final openAgent = unansweredAgentMessage(entries);
    final connection = chat.connectionFailure;
    final failure =
        chat.failure ??
        (chat.offline
            ? offlineChatFailure(
                message: connection == null
                    ? 'Sin conexión con ${chat.alfredName}.'
                    : connectionText(connection),
                action:
                    'Última sincronización '
                    '${relativeTime(chat.lastSync?.toIso8601String())}. '
                    'Lo que ya está cargado sigue legible; no se puede enviar.',
              )
            : null);
    final showFailure = failure != null && failure.shownInTimeline;
    if (chat.loadingConversation && entries.isEmpty) {
      return const Center(child: CircularProgressIndicator());
    }
    // Pending approvals this user must decide close the timeline as cards.
    final approvals = chat.approvals;
    final cards =
        approvals?.userPending(conversationId: chat.current?.id) ??
        const <ChatApproval>[];
    // A new conversation has only its welcome. The welcome has its own scroll view, so the
    // timeline's jump to the end never cuts the top of the portrait.
    if (entries.isEmpty && !showFailure && cards.isEmpty) {
      return SingleChildScrollView(
        padding: const EdgeInsets.only(
          top: Spacing.sm,
          bottom: _welcomeBottomInset,
        ),
        child: _Welcome(alfredName: chat.alfredName, starters: starters),
      );
    }
    return ListView.builder(
      controller: scroll,
      padding: const EdgeInsets.only(top: Spacing.sm, bottom: Spacing.md),
      itemCount:
          entries.length +
          (showFailure ? 1 : 0) +
          (entries.isEmpty ? 1 : 0) +
          cards.length,
      itemBuilder: (context, index) {
        var cursor = index;
        if (showFailure) {
          if (cursor == 0) {
            return ChatFailureCard(
              failure: failure,
              onRetry: onRetryFailure,
              onDismiss: chat.clearFailure,
            );
          }
          cursor--;
        }
        if (cursor < entries.length) {
          return TimelineRow(
            key: rowKey(entries[cursor].seq),
            entry: entries[cursor],
            agentName: chat.alfredName,
            runningAgent: chat.bootstrap?.alfredName,
            turnStep: chat.turnProgress,
            latestOwn: identical(entries[cursor], latest),
            latestAgent: identical(entries[cursor], openAgent),
            onReply: onReply,
            onOpenQuote: onOpenQuote,
            // A send under way disables the chips rather than hiding them.
            onSuggestion: chat.canSend ? onSuggestion : null,
            onRetrySend: onRetrySend,
            onRetryTurn: onRetryTurn,
            onOpenDelegation: onOpenDelegation,
            onOpenAgent: onOpenAgent,
            onCancelDelegation: onCancelDelegation,
            approvals: approvals,
            conversationId: chat.current?.id,
            projects: chat.projects,
            alfred: chat.alfredSeat,
            onOpenDraft: onOpenDraft,
            images: chat.images,
            documents: chat.documents,
            sentAttachments: chat.attachmentsOfSend(entries[cursor].clientId),
            replyBelow:
                cursor + 1 < entries.length &&
                entries[cursor + 1].kind == 'message' &&
                entries[cursor + 1].authorKind == 'agent',
            // The row above is the older entry: a run of Alfred's replies has tails only on its first.
            startsStreak:
                !(cursor > 0 &&
                    entries[cursor - 1].kind == 'message' &&
                    entries[cursor - 1].authorKind == 'agent'),
          );
        }
        cursor -= entries.length;
        if (entries.isEmpty) {
          if (cursor == 0) {
            return _Welcome(alfredName: chat.alfredName, starters: starters);
          }
          cursor--;
        }
        if (approvals == null) return const SizedBox.shrink();
        return Padding(
          padding: const EdgeInsets.fromLTRB(
            Spacing.lg,
            Spacing.sm,
            Spacing.lg,
            0,
          ),
          child: ApprovalCardHost(
            controller: approvals,
            approvalId: cards[cursor].approvalId,
            projects: chat.projects,
            alfred: chat.alfredSeat,
          ),
        );
      },
    );
  }
}
