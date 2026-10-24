import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/approvals/model.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';
import 'package:herdr_notify/src/rust/api/error/chat.dart' show offlineChatFailure;

import '../../core/state/chat_controller.dart';
import '../theme.dart';
import '../widgets/time_label.dart';
import 'approval_card.dart';
import 'chat_bubbles.dart';

class ConversationView extends StatelessWidget {
  const ConversationView({
    super.key,
    required this.chat,
    required this.scroll,
    required this.onRetrySend,
    required this.onRetryTurn,
    required this.onRetryFailure,
    required this.onOpenDelegation,
    required this.onOpenAgent,
    required this.onCancelDelegation,
  });

  final ChatController chat;
  final ScrollController scroll;
  final ValueChanged<ChatEntry> onRetrySend;
  final ValueChanged<ChatEntry> onRetryTurn;
  final VoidCallback onRetryFailure;
  final ValueChanged<ChatEntry> onOpenDelegation;
  final ValueChanged<ChatEntry> onOpenAgent;
  final ValueChanged<ChatEntry> onCancelDelegation;

  @override
  Widget build(BuildContext context) {
    final entries = chat.entries;
    final failure =
        chat.failure ??
        (chat.offline
            ? offlineChatFailure(
                message: 'Sin conexión con ${chat.alfredName}.',
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
            entry: entries[cursor],
            agentName: chat.alfredName,
            onRetrySend: onRetrySend,
            onRetryTurn: onRetryTurn,
            onOpenDelegation: onOpenDelegation,
            onOpenAgent: onOpenAgent,
            onCancelDelegation: onCancelDelegation,
            approvals: approvals,
            conversationId: chat.current?.id,
            projects: chat.projects,
          );
        }
        cursor -= entries.length;
        if (entries.isEmpty) {
          if (cursor == 0) {
            return Padding(
              padding: const EdgeInsets.all(Spacing.xl),
              child: Text(
                'Conversación nueva con ${chat.alfredName}. Escribe el primer mensaje.',
                style: context.text.bodySecondary,
                textAlign: TextAlign.center,
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
          ),
        );
      },
    );
  }
}
