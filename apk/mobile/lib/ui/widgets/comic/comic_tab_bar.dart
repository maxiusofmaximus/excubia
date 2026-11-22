import 'package:flutter/material.dart';

import '../../theme.dart';
import 'comic_slab.dart';

/// One destination of a [ComicTabBar]: its name and its icon, with a filled one when chosen.
class ComicTab {
  const ComicTab({
    required this.label,
    required this.icon,
    required this.selectedIcon,
  });

  final String label;
  final Widget icon;
  final Widget selectedIcon;
}

/// The bottom bar as leaning tabs. The chosen one is a hot slab with a ▶ marker;
/// the others are deep slabs. Each tab is a 48 dp or taller target.
class ComicTabBar extends StatelessWidget {
  const ComicTabBar({
    super.key,
    required this.tabs,
    required this.selected,
    required this.onSelected,
  });

  final List<ComicTab> tabs;
  final int selected;
  final ValueChanged<int> onSelected;

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: context.colors.bg,
      child: SafeArea(
        top: false,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(
            Spacing.md,
            Spacing.sm,
            Spacing.md,
            Spacing.sm,
          ),
          child: Row(
            children: [
              for (var i = 0; i < tabs.length; i++) ...[
                if (i > 0) const SizedBox(width: Spacing.sm),
                Expanded(
                  child: _ComicTabButton(
                    tab: tabs[i],
                    active: i == selected,
                    onTap: () => onSelected(i),
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

class _ComicTabButton extends StatelessWidget {
  const _ComicTabButton({
    required this.tab,
    required this.active,
    required this.onTap,
  });

  final ComicTab tab;
  final bool active;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final ink = active ? c.onHot : c.onDeep;
    return Semantics(
      button: true,
      selected: active,
      label: tab.label,
      excludeSemantics: true,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: ComicSlab(
          color: active ? c.hot : c.deep,
          shadow: active ? comicShadow : 2,
          padding: const EdgeInsets.symmetric(
            horizontal: Spacing.sm,
            vertical: Spacing.xs,
          ),
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 44),
            child: Center(
              heightFactor: 1,
              child: IconTheme(
                data: IconThemeData(color: ink, size: 20),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    if (active) ...[
                      const Icon(Icons.play_arrow, size: 14),
                      const SizedBox(width: Spacing.xs),
                    ],
                    active ? tab.selectedIcon : tab.icon,
                    const SizedBox(width: Spacing.xs),
                    Flexible(
                      child: Text(
                        tab.label,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: context.text.label.copyWith(
                          color: ink,
                          fontSize: 13,
                          fontWeight: FontWeight.w800,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
