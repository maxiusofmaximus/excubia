import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';
import 'package:herdr_notify/src/rust/api/error/chat.dart';

import '../../core/state/approvals_controller.dart';
import '../theme.dart';
import '../widgets/surface_card.dart';
import '../widgets/time_label.dart';
import 'approval_timeline_line.dart';
import 'delegation_card.dart';

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
  });

  final ChatEntry entry;
  final String agentName;
  final ValueChanged<ChatEntry> onRetrySend;
  final ValueChanged<ChatEntry> onRetryTurn;
  final ValueChanged<ChatEntry> onOpenDelegation;
  final ValueChanged<ChatEntry> onOpenAgent;
  final ValueChanged<ChatEntry> onCancelDelegation;
  final ApprovalsController? approvals;
  final String? conversationId;
  final List<ChatProject> projects;

  @override
  Widget build(BuildContext context) {
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
      onRetrySend: onRetrySend,
    );
  }
}

class _MessageBubble extends StatelessWidget {
  const _MessageBubble({
    required this.entry,
    required this.mine,
    required this.author,
    required this.onRetrySend,
  });

  final ChatEntry entry;
  final bool mine;
  final String author;
  final ValueChanged<ChatEntry> onRetrySend;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final sending = entry.local && entry.state == 'sending';
    final failed = entry.local && entry.state == 'failed';
    return Align(
      alignment: mine ? Alignment.centerRight : Alignment.centerLeft,
      child: Container(
        margin: EdgeInsets.fromLTRB(
          mine ? 40 : Spacing.lg,
          Spacing.xs,
          mine ? Spacing.lg : 40,
          Spacing.xs,
        ),
        padding: const EdgeInsets.fromLTRB(
          Spacing.md,
          Spacing.sm,
          Spacing.md,
          Spacing.sm,
        ),
        constraints: const BoxConstraints(maxWidth: 520),
        decoration: BoxDecoration(
          color: mine ? c.accentSoft : c.surface,
          borderRadius: BorderRadius.circular(14),
          border: Border.all(color: failed ? c.statusBlocked : c.outlineSoft),
        ),
        child: Column(
          crossAxisAlignment: mine
              ? CrossAxisAlignment.end
              : CrossAxisAlignment.start,
          children: [
            Text(
              author,
              style: context.text.label.copyWith(
                color: mine ? c.textSecondary : c.accent,
              ),
            ),
            const SizedBox(height: 2),
            Text(
              entry.body.isEmpty ? entry.preview : entry.body,
              style: context.text.body,
            ),
            if (sending || failed) ...[
              const SizedBox(height: Spacing.xs),
              if (sending)
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    SizedBox(
                      width: 12,
                      height: 12,
                      child: CircularProgressIndicator(
                        strokeWidth: 2,
                        color: c.textSecondary,
                      ),
                    ),
                    const SizedBox(width: Spacing.xs),
                    Text('Enviando…', style: context.text.small),
                  ],
                )
              else
                Wrap(
                  crossAxisAlignment: WrapCrossAlignment.center,
                  spacing: Spacing.xs,
                  children: [
                    Text(
                      'Falló el envío',
                      style: context.text.small.copyWith(
                        color: c.statusBlocked,
                      ),
                    ),
                    TextButton(
                      onPressed: () => onRetrySend(entry),
                      style: TextButton.styleFrom(
                        minimumSize: const Size(48, 48),
                        foregroundColor: c.accent,
                      ),
                      child: const Text('Reintentar'),
                    ),
                  ],
                ),
            ],
          ],
        ),
      ),
    );
  }
}

class _FailedTurnCard extends StatelessWidget {
  const _FailedTurnCard({required this.entry, required this.onRetry});

  final ChatEntry entry;
  final ValueChanged<ChatEntry> onRetry;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: Spacing.lg,
        vertical: Spacing.sm,
      ),
      child: SurfaceCard(
        borderColor: c.statusBlocked.withValues(alpha: 0.55),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(Icons.error_outline, size: 18, color: c.statusBlocked),
                const SizedBox(width: Spacing.sm),
                Expanded(
                  child: Text(
                    'Alfred terminó sin responder',
                    style: context.text.title.copyWith(color: c.statusBlocked),
                  ),
                ),
              ],
            ),
            const SizedBox(height: Spacing.xs),
            Text(
              'El turno falló antes de llamar a chat.reply; no se inventó una '
              'respuesta. El historial y las tarjetas se conservan.',
              style: context.text.bodySecondary,
            ),
            const SizedBox(height: Spacing.sm),
            Row(
              children: [
                OutlinedButton(
                  onPressed: () => onRetry(entry),
                  style: OutlinedButton.styleFrom(
                    minimumSize: const Size(48, 48),
                  ),
                  child: const Text('Reintentar'),
                ),
                const SizedBox(width: Spacing.sm),
                Text(
                  entry.createdAt.isEmpty ? '' : relativeTime(entry.createdAt),
                  style: context.text.small,
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _ActivityRow extends StatelessWidget {
  const _ActivityRow({required this.entry});

  final ChatEntry entry;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final label = entry.title ?? entry.preview;
    final detail = entry.summary;
    return Semantics(
      container: true,
      label: 'Actividad de la conversación',
      child: Container(
        margin: const EdgeInsets.symmetric(
          horizontal: Spacing.lg,
          vertical: Spacing.xs,
        ),
        padding: const EdgeInsets.all(Spacing.md),
        decoration: BoxDecoration(
          color: c.raised,
          borderRadius: BorderRadius.circular(12),
          border: Border.all(color: c.outlineSoft),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(Icons.article_outlined, size: 18, color: c.textMuted),
            const SizedBox(width: Spacing.sm),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    label.isEmpty ? 'Entrada de actividad' : label,
                    style: context.text.body,
                  ),
                  if (detail != null && detail.isNotEmpty) ...[
                    const SizedBox(height: 2),
                    Text(detail, style: context.text.small),
                  ],
                  if (entry.status != null) ...[
                    const SizedBox(height: Spacing.xs),
                    Text(
                      _statusLabel(entry.status!),
                      style: context.text.label.copyWith(color: c.textMuted),
                    ),
                  ],
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// `wf46` states, one card. The code selects the recovery, not the wording of
/// an arbitrary status number.
class ChatFailureCard extends StatelessWidget {
  const ChatFailureCard({
    super.key,
    required this.failure,
    required this.onRetry,
    required this.onDismiss,
  });

  final ChatFailure failure;
  final VoidCallback onRetry;
  final VoidCallback onDismiss;

  static const Set<String> _commandCodes = {'forbidden'};

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final permission = _commandCodes.contains(failure.code);
    final offline = failure.code == 'offline';
    final accent = permission || offline ? c.statusBlocked : c.accent;
    return Container(
      margin: const EdgeInsets.symmetric(
        horizontal: Spacing.lg,
        vertical: Spacing.sm,
      ),
      decoration: BoxDecoration(
        color: c.surface,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: c.outlineSoft),
      ),
      clipBehavior: Clip.antiAlias,
      child: IntrinsicHeight(
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Container(width: 3, color: accent),
            Expanded(
              child: Padding(
                padding: const EdgeInsets.all(Spacing.md),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Icon(Icons.error_outline, size: 18, color: accent),
                        const SizedBox(width: Spacing.sm),
                        Expanded(
                          child: Text(
                            failure.message,
                            style: context.text.body,
                          ),
                        ),
                        IconButton(
                          tooltip: 'Descartar aviso',
                          onPressed: onDismiss,
                          icon: const Icon(Icons.close, size: 18),
                          color: c.textMuted,
                        ),
                      ],
                    ),
                    if (failure.action != null &&
                        failure.action!.isNotEmpty) ...[
                      const SizedBox(height: Spacing.xs),
                      if (permission)
                        SelectableText(
                          failure.action!,
                          style: context.text.mono.copyWith(fontSize: 12),
                        )
                      else
                        Text(failure.action!, style: context.text.small),
                    ],
                    const SizedBox(height: Spacing.sm),
                    Wrap(
                      spacing: Spacing.sm,
                      runSpacing: Spacing.sm,
                      children: [
                        if (!permission)
                          FilledButton(
                            onPressed: onRetry,
                            style: FilledButton.styleFrom(
                              minimumSize: const Size(48, 48),
                            ),
                            child: Text(offline ? 'Reconectar' : 'Reintentar'),
                          ),
                        if (permission)
                          OutlinedButton.icon(
                            onPressed: () => Clipboard.setData(
                              ClipboardData(text: failure.action ?? ''),
                            ),
                            style: OutlinedButton.styleFrom(
                              minimumSize: const Size(48, 48),
                            ),
                            icon: const Icon(Icons.copy, size: 18),
                            label: const Text('Copiar comando'),
                          ),
                        TextButton(
                          onPressed: onDismiss,
                          style: TextButton.styleFrom(
                            minimumSize: const Size(48, 48),
                          ),
                          child: const Text('Cerrar'),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

String _statusLabel(String status) => switch (status) {
  'queued' => 'en cola',
  'running' => 'en curso',
  'waiting' => 'esperando',
  'done' => 'hecho',
  'failed' => 'falló',
  'cancelled' => 'cancelado',
  'interrupted' => 'interrumpido',
  _ => status,
};
