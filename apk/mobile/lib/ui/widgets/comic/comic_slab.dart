import 'package:flutter/material.dart';

import '../../theme.dart';

/// How far a slab leans: its top edge starts this far right of its bottom edge.
const double comicSlant = 6;

/// The hard shadow's offset, down and to the right.
const double comicShadow = 4;

/// The size of a cut corner.
const double _cut = 10;

/// How far the tail points out of the left edge.
const double _tail = 8;

/// A skewed piece with a hard shadow, the comic skin's one shape. Only the shape
/// leans; the child is laid out straight, inside the padding that clears the slant.
class ComicSlab extends StatelessWidget {
  const ComicSlab({
    super.key,
    required this.color,
    required this.child,
    this.stroke,
    this.cut = false,
    this.tail = false,
    this.shadow = comicShadow,
    this.padding = const EdgeInsets.symmetric(
      horizontal: Spacing.md,
      vertical: Spacing.sm,
    ),
  });

  final Color color;
  final Widget child;

  /// A 2 dp keyline inside the edge; null draws none.
  final Color? stroke;

  /// Cuts the top-right corner.
  final bool cut;

  /// A point on the left edge near the top, toward the face it belongs to.
  final bool tail;

  /// The hard shadow's offset; 0 draws none.
  final double shadow;

  /// Space between the edge and the child, before the slant is added.
  final EdgeInsetsGeometry padding;

  @override
  Widget build(BuildContext context) {
    final pad = padding.resolve(TextDirection.ltr);
    final lead = tail ? _tail : 0.0;
    return CustomPaint(
      painter: _SlabPainter(
        color: color,
        stroke: stroke,
        shadowColor: context.colors.hard,
        cut: cut,
        tail: tail,
        shadow: shadow,
      ),
      child: Padding(
        padding: EdgeInsets.fromLTRB(
          lead + comicSlant + pad.left,
          pad.top,
          comicSlant + pad.right + shadow,
          pad.bottom + shadow,
        ),
        child: child,
      ),
    );
  }
}

/// The outline of a slab in [rect]: a leaning top, an optional cut corner and an
/// optional tail on the left edge, near the top.
Path comicSlabPath(Rect rect, {bool cut = false, bool tail = false}) {
  final left = rect.left;
  final top = rect.top;
  final right = rect.right;
  final bottom = rect.bottom;
  // The x of the left edge at height y: it runs from (left, bottom) to (left + slant, top).
  double edgeAt(double y) => left + comicSlant * (bottom - y) / rect.height;
  final path = Path()..moveTo(left + comicSlant, top);
  if (cut) {
    path
      ..lineTo(right - _cut, top)
      ..lineTo(right, top + _cut);
  } else {
    path.lineTo(right, top);
  }
  path
    ..lineTo(right - comicSlant, bottom)
    ..lineTo(left, bottom);
  if (tail) {
    const upper = 12.0;
    const lower = 26.0;
    const middle = (upper + lower) / 2;
    path
      ..lineTo(edgeAt(top + lower), top + lower)
      ..lineTo(edgeAt(top + middle) - _tail, top + middle)
      ..lineTo(edgeAt(top + upper), top + upper);
  }
  return path..close();
}

class _SlabPainter extends CustomPainter {
  const _SlabPainter({
    required this.color,
    required this.stroke,
    required this.shadowColor,
    required this.cut,
    required this.tail,
    required this.shadow,
  });

  final Color color;
  final Color? stroke;
  final Color shadowColor;
  final bool cut;
  final bool tail;
  final double shadow;

  @override
  void paint(Canvas canvas, Size size) {
    final lead = tail ? _tail : 0.0;
    final rect = Rect.fromLTRB(
      lead,
      0,
      size.width - shadow,
      size.height - shadow,
    );
    final path = comicSlabPath(rect, cut: cut, tail: tail);
    if (shadow > 0) {
      canvas.drawPath(
        path.shift(Offset(shadow, shadow)),
        Paint()..color = shadowColor,
      );
    }
    canvas.drawPath(path, Paint()..color = color);
    if (stroke case final ink?) {
      canvas.drawPath(
        path,
        Paint()
          ..color = ink
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2,
      );
    }
  }

  @override
  bool shouldRepaint(_SlabPainter old) =>
      old.color != color ||
      old.stroke != stroke ||
      old.shadowColor != shadowColor ||
      old.cut != cut ||
      old.tail != tail ||
      old.shadow != shadow;
}
