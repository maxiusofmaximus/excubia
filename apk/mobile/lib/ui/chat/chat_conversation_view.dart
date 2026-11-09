import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/approvals/model.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';
import 'package:herdr_notify/src/rust/api/error/chat.dart'
    show offlineChatFailure;

import '../../core/api/connection_text.dart';
import '../../core/state/chat_controller.dart';
import '../theme.dart';
import '../widgets/time_label.dart';
import 'approval_card.dart';
import 'chat_bubbles.dart';

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
          );
        }
        cursor -= entries.length;
        if (entries.isEmpty) {
          if (cursor == 0) {
            return Padding(
              padding: const EdgeInsets.all(Spacing.xl),
              child: Column(
                children: [
                  Text(
                    'Conversación nueva con ${chat.alfredName}. Escribe el primer mensaje.',
                    style: context.text.bodySecondary,
                    textAlign: TextAlign.center,
                  ),
                  const SizedBox(height: Spacing.lg),
                  starters,
                ],
              ),
            );
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
