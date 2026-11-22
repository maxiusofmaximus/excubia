import 'package:flutter/material.dart';

import '../../theme.dart';
import 'comic_slab.dart';

/// Which role a [ComicTag] takes: the paper, the accent or the deep surface.
enum ComicTagTone { pale, hot, deep }

/// A short stamp-like label on a small slab. It is set in capitals unless [caps]
/// is off; a screen reader hears the text as written.
class ComicTag extends StatelessWidget {
  const ComicTag({
    super.key,
    required this.label,
    this.tone = ComicTagTone.pale,
    this.caps = true,
  });

  final String label;
  final ComicTagTone tone;

  /// Off keeps the text as written, for a stamp that must read as it is.
  final bool caps;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final (Color fill, Color ink) = switch (tone) {
      ComicTagTone.pale => (c.pale, c.onPale),
      ComicTagTone.hot => (c.hot, c.onHot),
      ComicTagTone.deep => (c.deep, c.onDeep),
    };
    return Semantics(
      label: label,
      excludeSemantics: true,
      child: ComicSlab(
        color: fill,
        shadow: 2,
        padding: const EdgeInsets.symmetric(
          horizontal: Spacing.sm,
          vertical: 2,
        ),
        child: Text(
          caps ? label.toUpperCase() : label,
          style: context.text.label.copyWith(
            color: ink,
            fontSize: 12,
            fontWeight: FontWeight.w800,
            letterSpacing: 0.8,
          ),
        ),
      ),
    );
  }
}
