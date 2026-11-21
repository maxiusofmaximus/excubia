import 'package:flutter/material.dart';

import '../../theme.dart';
import 'comic_slab.dart';

/// Which answer a [ComicButton] gives: the main one, the other one, or the one that stops.
enum ComicButtonTone { primary, secondary, danger }

/// The desktop's button: a hot, deep or red slab with a hard shadow. The text
/// stays straight; the slab leans. Disabled, it is an outline, not a faded slab.
/// It keeps a 48 dp touch target.
class ComicButton extends StatelessWidget {
  const ComicButton({
    super.key,
    required this.label,
    this.onPressed,
    this.tone = ComicButtonTone.primary,
    this.icon,
    this.square = false,
    this.busy = false,
    this.tooltip,
  });

  /// The visible text, or the name a screen reader gives an icon-only button.
  final String label;
  final VoidCallback? onPressed;
  final ComicButtonTone tone;

  /// Leads the label; alone it is the whole face of a [square] button.
  final Widget? icon;

  /// A 48 dp square for an icon-only button.
  final bool square;

  /// A spinner in place of the content. The button keeps its label and takes no taps.
  final bool busy;
  final String? tooltip;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final off = onPressed == null;
    final enabled = !off && !busy;
    final (Color fill, Color ink, Color? edge) = off
        ? (c.surface, c.textMuted, c.outlineStrong)
        : switch (tone) {
            ComicButtonTone.primary => (c.hot, c.onHot, null),
            ComicButtonTone.secondary => (c.deep, c.onDeep, c.outlineStrong),
            ComicButtonTone.danger => (c.statusBlocked, c.onAttention, null),
          };
    final Widget content;
    if (busy) {
      content = SizedBox.square(
        dimension: 18,
        child: CircularProgressIndicator(strokeWidth: 2, color: ink),
      );
    } else if (square) {
      content = icon ?? const SizedBox.shrink();
    } else {
      content = Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (icon != null) ...[icon!, const SizedBox(width: Spacing.xs)],
          // One line: a long label scales down inside the slab rather than wrapping.
          Flexible(
            child: FittedBox(
              fit: BoxFit.scaleDown,
              child: Text(
                label,
                textAlign: TextAlign.center,
                maxLines: 1,
                style: context.text.label.copyWith(
                  color: ink,
                  fontSize: 14,
                  fontWeight: FontWeight.w800,
                  letterSpacing: 0.4,
                ),
              ),
            ),
          ),
        ],
      );
    }
    // The minimum sizes are the 48 dp target less the slab's own shadow and padding.
    final inner = ConstrainedBox(
      constraints: BoxConstraints(
        minHeight: square
            ? 48 - comicShadow
            : 48 - comicShadow - 2 * Spacing.sm,
        minWidth: square ? 48 - comicSlant - comicShadow : 0,
      ),
      child: Center(
        heightFactor: 1,
        child: IconTheme(
          data: IconThemeData(color: ink, size: 20),
          child: content,
        ),
      ),
    );
    final face = ComicSlab(
      color: fill,
      stroke: edge,
      padding: square
          ? EdgeInsets.zero
          : const EdgeInsets.symmetric(
              horizontal: Spacing.lg,
              vertical: Spacing.sm,
            ),
      child: inner,
    );
    Widget result = Semantics(
      button: true,
      enabled: enabled,
      label: label,
      excludeSemantics: true,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: enabled ? onPressed : null,
        child: face,
      ),
    );
    if (tooltip case final message?) {
      result = Tooltip(message: message, child: result);
    }
    return result;
  }
}
