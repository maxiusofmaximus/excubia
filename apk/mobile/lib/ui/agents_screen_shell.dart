part of 'agents_screen.dart';

class _Shell extends StatelessWidget {
  const _Shell({
    required this.subtitle,
    required this.child,
    this.onRefresh,
    this.onNewConversation,
  });

  final String subtitle;
  final Widget child;
  final VoidCallback? onRefresh;
  final VoidCallback? onNewConversation;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: context.colors.bg,
      body: SafeArea(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(
                Spacing.lg,
                Spacing.lg,
                Spacing.sm,
                Spacing.md,
              ),
              child: Row(
                children: [
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text('Equipo', style: context.text.headline),
                        const SizedBox(height: 2),
                        Text(subtitle, style: context.text.small),
                      ],
                    ),
                  ),
                  IconButton(
                    tooltip: 'Nueva conversación',
                    onPressed: onNewConversation,
                    icon: const Icon(Icons.add_comment_outlined),
                    color: context.colors.textSecondary,
                  ),
                  IconButton(
                    tooltip: 'Actualizar',
                    onPressed: onRefresh,
                    icon: const Icon(Icons.refresh),
                    color: context.colors.textSecondary,
                  ),
                ],
              ),
            ),
            Expanded(child: child),
          ],
        ),
      ),
    );
  }
}

class _OfflineBanner extends StatelessWidget {
  const _OfflineBanner({
    required this.reason,
    required this.updatedAt,
    required this.failure,
  });

  final ConnectionFailure? failure;

  final String? reason;
  final DateTime? updatedAt;

  @override
  Widget build(BuildContext context) {
    final failure = this.failure;
    return Container(
      margin: const EdgeInsets.fromLTRB(Spacing.lg, 0, Spacing.lg, Spacing.sm),
      padding: const EdgeInsets.all(Spacing.md),
      decoration: BoxDecoration(
        color: context.colors.raised,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: context.colors.outlineSoft),
      ),
      child: Row(
        children: [
          Icon(
            Icons.cloud_off_outlined,
            size: 18,
            color: context.colors.textSecondary,
          ),
          const SizedBox(width: Spacing.sm),
          Expanded(
            child: Text(
              failure == null
                  ? 'Sin conexión — '
                        '${reason == null || reason!.isEmpty ? 'mostrando el último estado' : reason} '
                        '(${ageLabel(updatedAt)}).'
                  : '${connectionText(failure)} (${ageLabel(updatedAt)}).',
              style: context.text.small,
            ),
          ),
        ],
      ),
    );
  }
}
