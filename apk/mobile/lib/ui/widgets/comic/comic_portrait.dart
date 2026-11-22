import 'package:flutter/material.dart';

import '../../theme.dart';
import 'comic_slab.dart';

/// A character's frame, as the desktop draws it: a square whose top right corner
/// is cut, with a pale keyline 3 dp inside the edge and a hard shadow. The picture
/// fills the keyline with the same cut. Nothing leans and nothing sticks out.
class ComicPortrait extends StatelessWidget {
  const ComicPortrait({
    super.key,
    required this.size,
    required this.panel,
    required this.face,
  });

  /// The whole box, shadow included.
  final double size;

  /// The fill behind the picture.
  final Color panel;

  /// The square picture.
  final Widget face;

  static const _keyline = 3.0;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final frame = size - comicShadow;
    final cut = frame * 0.16;
    return SizedBox.square(
      dimension: size,
      child: Stack(
        children: [
          Positioned.fill(
            child: CustomPaint(
              painter: _FramePainter(keyline: c.pale, shadow: c.hard, cut: cut),
            ),
          ),
          Positioned(
            left: _keyline,
            top: _keyline,
            width: frame - 2 * _keyline,
            height: frame - 2 * _keyline,
            child: ClipPath(
              clipper: _CutClipper(cut),
              child: ColoredBox(color: panel, child: face),
            ),
          ),
        ],
      ),
    );
  }
}

/// A square with its top right corner cut by [cut].
Path _cutSquare(Rect rect, double cut) => Path()
  ..moveTo(rect.left, rect.top)
  ..lineTo(rect.right - cut, rect.top)
  ..lineTo(rect.right, rect.top + cut)
  ..lineTo(rect.right, rect.bottom)
  ..lineTo(rect.left, rect.bottom)
  ..close();

/// The keyline and the hard shadow. The keyline's cut sits 2 dp outside the
/// picture's, so the outline stays 3 dp wide along the diagonal too.
class _FramePainter extends CustomPainter {
  const _FramePainter({
    required this.keyline,
    required this.shadow,
    required this.cut,
  });

  final Color keyline;
  final Color shadow;
  final double cut;

  @override
  void paint(Canvas canvas, Size size) {
    final frame = Rect.fromLTWH(
      0,
      0,
      size.width - comicShadow,
      size.height - comicShadow,
    );
    final path = _cutSquare(frame, cut + 2);
    canvas.drawPath(
      path.shift(const Offset(comicShadow, comicShadow)),
      Paint()..color = shadow,
    );
    canvas.drawPath(path, Paint()..color = keyline);
  }

  @override
  bool shouldRepaint(_FramePainter old) =>
      old.keyline != keyline || old.shadow != shadow || old.cut != cut;
}

class _CutClipper extends CustomClipper<Path> {
  const _CutClipper(this.cut);

  final double cut;

  @override
  Path getClip(Size size) => _cutSquare(Offset.zero & size, cut);

  @override
  bool shouldReclip(_CutClipper old) => old.cut != cut;
}
