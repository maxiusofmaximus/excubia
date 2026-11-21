import 'package:flutter/material.dart';

import '../../theme.dart';
import 'comic_slab.dart';

/// A switch as a leaning track with a square thumb. The track's keyline carries
/// the 3:1 against the page in every palette, so the off state stays visible.
class ComicSwitch extends StatelessWidget {
  const ComicSwitch({
    super.key,
    required this.value,
    required this.onChanged,
    required this.semanticLabel,
  });

  final bool value;

  /// Null disables the switch: it shows the same track, muted, and takes no taps.
  final ValueChanged<bool>? onChanged;
  final String semanticLabel;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final off = onChanged == null;
    final fill = off ? c.surface : (value ? c.hot : c.surface3);
    final edge = value ? c.accentText : c.outlineStrong;
    final thumb = off ? c.outlineStrong : (value ? c.onHot : c.textSecondary);
    return Semantics(
      toggled: value,
      enabled: !off,
      label: semanticLabel,
      excludeSemantics: true,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: off ? null : () => onChanged!(!value),
        child: SizedBox(
          width: 64,
          height: 48,
          child: Center(
            child: ComicSlab(
              color: fill,
              stroke: edge,
              shadow: 0,
              padding: EdgeInsets.zero,
              child: SizedBox(
                width: 52,
                height: 26,
                child: Align(
                  alignment: value
                      ? Alignment.centerRight
                      : Alignment.centerLeft,
                  child: Padding(
                    padding: const EdgeInsets.all(5),
                    child: SizedBox.square(
                      dimension: 16,
                      child: ColoredBox(color: thumb),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
