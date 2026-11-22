import 'package:flutter/material.dart';

import '../theme.dart';
import '../widgets/comic/comic_slab.dart';

/// The Equipo search as a comic slab: the field inside has no border and no fill of its own.
class TeamSearchPanel extends StatelessWidget {
  const TeamSearchPanel({
    super.key,
    required this.controller,
    required this.onChanged,
  });

  final TextEditingController controller;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return ComicSlab(
      color: c.pale,
      shadow: 2,
      padding: const EdgeInsets.symmetric(
        horizontal: Spacing.sm,
        vertical: Spacing.xs,
      ),
      child: Row(
        children: [
          Icon(Icons.search, size: 20, color: c.onPale),
          const SizedBox(width: Spacing.sm),
          Expanded(
            child: TextField(
              controller: controller,
              onChanged: onChanged,
              cursorColor: c.onPale,
              style: context.text.body.copyWith(color: c.onPale),
              decoration: InputDecoration(
                hintText: 'Buscar agente o tarea…',
                hintStyle: context.text.body.copyWith(color: c.onPale),
                border: InputBorder.none,
                enabledBorder: InputBorder.none,
                focusedBorder: InputBorder.none,
                disabledBorder: InputBorder.none,
                filled: false,
                isDense: true,
                contentPadding: const EdgeInsets.symmetric(
                  vertical: Spacing.sm,
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// One filter tab: a leaning slab, the chosen one hot with a ▶ marker, the others deep.
/// Its touch target is at least 48 dp.
class TeamFilterTab extends StatelessWidget {
  const TeamFilterTab({
    super.key,
    required this.label,
    required this.selected,
    required this.onTap,
  });

  final String label;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final (Color fill, Color ink) = selected
        ? (c.hot, c.onHot)
        : (c.deep, c.onDeep);
    return Semantics(
      button: true,
      selected: selected,
      label: label,
      excludeSemantics: true,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: ConstrainedBox(
          constraints: const BoxConstraints(minWidth: 48, minHeight: 48),
          child: Center(
            widthFactor: 1,
            child: ComicSlab(
              color: fill,
              shadow: 2,
              padding: const EdgeInsets.symmetric(
                horizontal: Spacing.md,
                vertical: Spacing.xs,
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (selected) ...[
                    Icon(Icons.play_arrow, size: 14, color: ink),
                    const SizedBox(width: Spacing.xs),
                  ],
                  Flexible(
                    child: Text(
                      label,
                      maxLines: 1,
                      softWrap: false,
                      overflow: TextOverflow.ellipsis,
                      style: context.text.label.copyWith(
                        color: ink,
                        fontSize: 13,
                        fontWeight: FontWeight.w800,
                        letterSpacing: 0.4,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
