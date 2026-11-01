part of 'history_drawer.dart';

extension _HistoryDrawerFilters on _HistoryDrawerState {
  static const _labels = <HistoryFilter, String>{
    HistoryFilter.all: 'Todos',
    HistoryFilter.attention: 'Pendientes de ti',
    HistoryFilter.working: 'Trabajando',
    HistoryFilter.pinned: 'Fijados',
    HistoryFilter.sections: 'Secciones',
    HistoryFilter.archived: 'Archivados',
  };

  /// The two chips an older host cannot answer stay out of the row.
  static const _newFilters = {HistoryFilter.attention, HistoryFilter.working};

  /// Sections has no number, as before; the others read the host's chip counts.
  int? _chipCount(HistoryFilter filter) {
    final counts = _chat.counts;
    return switch (filter) {
      HistoryFilter.all => counts?.all.toInt(),
      HistoryFilter.attention => counts?.attention.toInt(),
      HistoryFilter.working => counts?.working.toInt(),
      HistoryFilter.pinned => _chat.pinnedCount,
      HistoryFilter.archived => _chat.archivedCount,
      HistoryFilter.sections => null,
    };
  }

  Widget _filters(BuildContext context) {
    return SizedBox(
      height: 56,
      child: ListView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: Spacing.md),
        children: [
          for (final filter in _labels.keys)
            if (_chat.countsSupported || !_newFilters.contains(filter))
              Padding(
                padding: const EdgeInsets.only(right: Spacing.sm),
                child: _chip(context, filter),
              ),
        ],
      ),
    );
  }

  Widget _chip(BuildContext context, HistoryFilter filter) {
    final c = context.colors;
    final label = _labels[filter]!;
    final count = _chipCount(filter) ?? 0;
    return Semantics(
      button: true,
      selected: _chat.filter == filter,
      label: count > 0 ? '$label, $count' : label,
      onTap: () => _chat.setFilter(filter),
      child: ExcludeSemantics(
        child: ChoiceChip(
          label: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(label),
              if (count > 0) ...[
                const SizedBox(width: Spacing.xs),
                Text(count > 99 ? '99+' : '$count'),
              ],
            ],
          ),
          selected: _chat.filter == filter,
          onSelected: (_) => _chat.setFilter(filter),
          showCheckmark: true,
          materialTapTargetSize: MaterialTapTargetSize.padded,
          labelStyle: context.text.small,
          selectedColor: c.accentSoft,
          backgroundColor: c.raised,
          side: BorderSide(color: c.outlineSoft),
          padding: const EdgeInsets.symmetric(
            horizontal: Spacing.sm,
            vertical: Spacing.md,
          ),
        ),
      ),
    );
  }
}
