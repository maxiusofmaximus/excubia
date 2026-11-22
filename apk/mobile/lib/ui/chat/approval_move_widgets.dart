part of 'approval_card.dart';

/// The batch of a `mail.move`: one row per mail, as text. A pending card gives
/// each row a checkbox; a decided card keeps the rows and marks the ones left out.
class _MoveBatch extends StatelessWidget {
  const _MoveBatch({
    required this.items,
    required this.unchecked,
    this.onToggle,
  });

  final List<MailMoveItem> items;
  final Set<String> unchecked;
  final void Function(String messageId, bool checked)? onToggle;

  @override
  Widget build(BuildContext context) {
    // A transparent Material lets the rows' ink reach the card's own background.
    return Material(
      type: MaterialType.transparency,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('${items.length} correos a mover', style: context.text.title),
          for (final item in items)
            _MoveRow(
              item: item,
              checked: !unchecked.contains(item.messageId),
              onToggle: onToggle,
            ),
        ],
      ),
    );
  }
}

class _MoveRow extends StatelessWidget {
  const _MoveRow({required this.item, required this.checked, this.onToggle});

  final MailMoveItem item;
  final bool checked;
  final void Function(String messageId, bool checked)? onToggle;

  @override
  Widget build(BuildContext context) {
    final text = context.text;
    final toggle = onToggle;
    final facts = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          item.fromAddress ?? 'Remitente desconocido',
          style: text.body,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        ),
        Text(
          item.subject.isEmpty ? 'Sin asunto' : item.subject,
          style: text.bodySecondary,
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
        ),
        Text(
          [
            destLabel(item.dest),
            triageLabel(item.level),
          ].whereType<String>().join(' · '),
          style: text.small,
        ),
        if (item.reason.isNotEmpty) Text(item.reason, style: text.small),
        if (item.excluded) Text('no se mueve', style: text.label),
      ],
    );
    if (toggle == null || item.excluded) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: Spacing.xs),
        child: facts,
      );
    }
    return MergeSemantics(
      child: CheckboxListTile(
        value: checked,
        onChanged: (value) => toggle(item.messageId, value ?? false),
        controlAffinity: ListTileControlAffinity.leading,
        contentPadding: EdgeInsets.zero,
        title: facts,
      ),
    );
  }
}
