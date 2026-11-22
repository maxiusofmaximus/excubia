import 'package:flutter/material.dart';

import 'surface_card.dart';

/// Rows in one flat card, separated by hairlines. Each row keeps its own ink,
/// clipped to the card's corners.
class GroupedCard extends StatelessWidget {
  const GroupedCard({super.key, required this.children});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) => SurfaceCard(
    padding: EdgeInsets.zero,
    child: Material(
      color: Colors.transparent,
      borderRadius: BorderRadius.circular(16),
      clipBehavior: Clip.antiAlias,
      child: Column(
        children: [
          for (final (index, child) in children.indexed) ...[
            if (index > 0) const Divider(height: 1),
            child,
          ],
        ],
      ),
    ),
  );
}
