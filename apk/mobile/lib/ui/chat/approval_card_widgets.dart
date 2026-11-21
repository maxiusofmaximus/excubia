part of 'approval_card.dart';

/// The monospaced command, or the host's question in body text when it asked.
class _CommandBox extends StatelessWidget {
  const _CommandBox({required this.line, required this.plain});

  final String line;
  final bool plain;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(
        horizontal: Spacing.md,
        vertical: Spacing.sm,
      ),
      decoration: BoxDecoration(
        color: colors.surface,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Text(line, style: plain ? context.text.body : context.text.mono),
    );
  }
}

/// The 403 without `APPROVE`: the canonical host line, selectable to copy, and
/// the recheck for a grant made since.
class _GrantNotice extends StatelessWidget {
  const _GrantNotice({required this.command, this.onRecheck});

  final String command;
  final VoidCallback? onRecheck;

  @override
  Widget build(BuildContext context) {
    final text = context.text;
    final colors = context.colors;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'Este dispositivo no puede aprobar. En el PC, ejecuta:',
          style: text.bodySecondary,
        ),
        const SizedBox(height: Spacing.xs),
        Container(
          width: double.infinity,
          padding: const EdgeInsets.all(Spacing.sm),
          decoration: BoxDecoration(
            color: colors.surface,
            borderRadius: BorderRadius.circular(8),
          ),
          child: SelectableText(command, style: text.mono),
        ),
        if (onRecheck != null) ...[
          const SizedBox(height: Spacing.sm),
          OutlinedButton(
            style: OutlinedButton.styleFrom(minimumSize: const Size(0, 48)),
            onPressed: onRecheck,
            child: const Text('Ya lo he concedido'),
          ),
        ],
      ],
    );
  }
}

/// «Detalles técnicos»: the host's tool name and the exact command, closed by
/// default so the card reads as one sentence.
class _TechnicalDetails extends StatelessWidget {
  const _TechnicalDetails({required this.command, required this.summary});

  final String command;
  final String summary;

  @override
  Widget build(BuildContext context) {
    return ExpansionTile(
      title: Text('Detalles técnicos', style: context.text.small),
      tilePadding: EdgeInsets.zero,
      childrenPadding: const EdgeInsets.only(bottom: Spacing.sm),
      expandedCrossAxisAlignment: CrossAxisAlignment.start,
      shape: const Border(),
      collapsedShape: const Border(),
      children: [
        if (summary.isNotEmpty) ...[
          Text(summary, style: context.text.small),
          const SizedBox(height: Spacing.xs),
        ],
        _CommandBox(line: command, plain: false),
      ],
    );
  }
}

class _Chip extends StatelessWidget {
  const _Chip({required this.label, this.icon});

  final String label;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: Spacing.sm,
        vertical: Spacing.xs,
      ),
      decoration: BoxDecoration(
        color: colors.surface3,
        borderRadius: BorderRadius.circular(999),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (icon != null) ...[
            Icon(icon, size: 14, color: colors.textSecondary),
            const SizedBox(width: Spacing.xs),
          ],
          Text(label, style: context.text.label),
        ],
      ),
    );
  }
}
