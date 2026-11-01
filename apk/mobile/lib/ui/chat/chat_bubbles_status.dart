part of 'chat_bubbles.dart';

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
                    style: context.text.title,
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
    final accent = permission || offline ? c.statusBlocked : c.accentText;
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
