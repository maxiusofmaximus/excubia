import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:excubia/src/rust/api/chat/attach_model.dart';
import 'package:excubia/src/rust/api/chat/model.dart';
import 'package:excubia/src/rust/api/chat/model_team.dart';
import 'package:excubia/src/rust/api/error/chat.dart';

import '../../core/state/approvals_controller.dart';
import '../../core/state/chat_attachment_images.dart';
import '../../core/state/chat_document_opener.dart';
import '../mail/mail_code_card.dart';
import '../mail/mail_draft_card.dart';
import '../theme.dart';
import '../widgets/surface_card.dart';
import '../widgets/time_label.dart';
import 'approval_timeline_line.dart';
import 'chat_attachments_view.dart';
import 'chat_replies.dart';
import 'delegation_card.dart';
import 'markdown_text.dart';

part 'chat_bubbles_message.dart';
part 'chat_bubbles_status.dart';

/// One timeline row: messages, delegation cards, the approval line of what the
/// user did not decide, and a neutral fallback for unknown activity. The
/// pending approval cards are placed by the chat screen, not here.
class TimelineRow extends StatelessWidget {
  const TimelineRow({
    super.key,
    required this.entry,
    required this.agentName,
    required this.onRetrySend,
    required this.onRetryTurn,
    required this.onOpenDelegation,
    required this.onOpenAgent,
    required this.onCancelDelegation,
    this.approvals,
    this.conversationId,
    this.projects = const <ChatProject>[],
    this.alfred,
    this.onOpenDraft,
    this.runningAgent,
    this.turnStep,
    this.latestOwn = false,
    this.latestAgent = false,
    this.onReply,
    this.onOpenQuote,
    this.onSuggestion,
    this.images,
    this.documents,
    this.sentAttachments,
  });

  final ChatEntry entry;
  final String agentName;

  /// The agent's name once the host has sent it; null means «Atendiendo».
  final String? runningAgent;

  /// The step Alfred set on the open turn; shown under the newest own message.
  final String? turnStep;

  /// True for the newest message the operator wrote: only it shows every step.
  final bool latestOwn;

  /// True for the newest agent message the operator has not answered yet: only
  /// it shows reply chips.
  final bool latestAgent;

  /// Starts a reply to this message; null where a message cannot be replied to.
  final ValueChanged<ChatEntry>? onReply;

  /// Scrolls to the message with this id; null disables the quote button.
  final ValueChanged<String>? onOpenQuote;

  /// Sends a chip's text; null shows the chips disabled.
  final ValueChanged<String>? onSuggestion;

  /// The timeline's image cache; null leaves the thumbnails out.
  final ChatAttachmentImages? images;

  /// Opens the documents of the timeline; null leaves their cards out.
  final ChatDocumentOpener? documents;

  /// The files of an unsent message, shown until the host's entry carries them.
  final List<ChatAttachment>? sentAttachments;
  final ValueChanged<ChatEntry> onRetrySend;
  final ValueChanged<ChatEntry> onRetryTurn;
  final ValueChanged<ChatEntry> onOpenDelegation;
  final ValueChanged<ChatEntry> onOpenAgent;
  final ValueChanged<ChatEntry> onCancelDelegation;

  /// Opens the draft sheet of a «Revisar borrador» row.
  final ValueChanged<ChatEntry>? onOpenDraft;
  final ApprovalsController? approvals;
  final String? conversationId;
  final List<ChatProject> projects;
  final ChatSeat? alfred;

  @override
  Widget build(BuildContext context) {
    if (entry.mail case final mail?) {
      // The host removes this card when the sign-in ends, so it has no check.
      return Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: Spacing.lg,
          vertical: Spacing.xs,
        ),
        child: MailCodeCard(
          title: 'Conecta tu correo · ${mail.account}',
          verificationUri: mail.verificationUri,
          userCode: mail.userCode,
          expiresAt: DateTime.tryParse(mail.expiresAt),
        ),
      );
    }
    if (entry.mailDraft case final draft?) {
      final open = onOpenDraft;
      return Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: Spacing.lg,
          vertical: Spacing.xs,
        ),
        child: DraftReviewCard(
          draft: draft,
          preview: entry.preview,
          onOpen: open == null ? null : () => open(entry),
        ),
      );
    }
    if (entry.kind == 'card') {
      return DelegationCard(
        entry: entry,
        onOpenAgent: onOpenAgent,
        onOpenDetail: onOpenDelegation,
        onCancel: onCancelDelegation,
      );
    }
    if (entry.kind == 'approval') {
      return ApprovalTimelineLine(
        entry: entry,
        controller: approvals,
        conversationId: conversationId,
        projects: projects,
        alfred: alfred,
      );
    }
    if (entry.kind != 'message') {
      return _ActivityRow(entry: entry);
    }
    final failedTurn =
        entry.authorKind == 'agent' &&
        entry.state == 'failed' &&
        entry.body.trim().isEmpty;
    if (failedTurn) return _FailedTurnCard(entry: entry, onRetry: onRetryTurn);
    final mine = entry.authorKind == 'user';
    return _MessageBubble(
      entry: entry,
      mine: mine,
      author: mine
          ? 'Tú'
          : (entry.authorKind == 'agent' ? agentName : 'Sistema'),
      agentName: agentName,
      onRetrySend: onRetrySend,
      latestOwn: latestOwn,
      runningAgent: runningAgent,
      turnStep: latestOwn ? turnStep : null,
      suggestions: latestAgent ? entry.suggestions : const <String>[],
      onSuggestion: onSuggestion,
      onReply: onReply,
      onOpenQuote: onOpenQuote,
      attachments: entry.attachments ?? sentAttachments,
      images: images,
      documents: documents,
    );
  }
}
