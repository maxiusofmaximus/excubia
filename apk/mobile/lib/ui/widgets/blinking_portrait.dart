import 'dart:async';
import 'dart:math';

import 'package:flutter/material.dart';

/// Blinks a portrait: every 3 to 6 s the [blink] render replaces [base] for
/// 120 ms. Only while [enabled], animations are on and the route is in front;
/// otherwise it is still, and the timer is gone.
class BlinkingPortrait extends StatefulWidget {
  const BlinkingPortrait({
    super.key,
    required this.base,
    required this.blink,
    required this.fallback,
    this.enabled = true,
    this.random,
  });

  /// Asset paths of the open-eyed render and the closed-eyes render.
  final String base;
  final String blink;

  /// Shown in place of [base] when its file is missing.
  final Widget fallback;
  final bool enabled;

  /// Injected by tests to fix the gaps; the default draws them at random.
  final Random? random;

  @override
  State<BlinkingPortrait> createState() => _BlinkingPortraitState();
}

class _BlinkingPortraitState extends State<BlinkingPortrait> {
  static const _blinkFor = Duration(milliseconds: 120);

  late final Random _random = widget.random ?? Random();
  Timer? _timer;
  bool _shut = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (widget.enabled) {
      unawaited(
        precacheImage(
          AssetImage(widget.blink),
          context,
        ).catchError((Object _) {}),
      );
    }
    _sync();
  }

  @override
  void didUpdateWidget(BlinkingPortrait old) {
    super.didUpdateWidget(old);
    _sync();
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  /// Starts the blinking when it is wanted, stops it when not. The field
  /// writes need no setState: build follows the dependency and update calls.
  void _sync() {
    final wanted =
        widget.enabled &&
        !MediaQuery.disableAnimationsOf(context) &&
        TickerMode.valuesOf(context).enabled;
    if (wanted) {
      _timer ??= Timer(_gap(), _blink);
    } else {
      _timer?.cancel();
      _timer = null;
      _shut = false;
    }
  }

  Duration _gap() => Duration(milliseconds: 3000 + _random.nextInt(3001));

  void _blink() {
    if (!mounted) return;
    setState(() => _shut = true);
    _timer = Timer(_blinkFor, () {
      if (!mounted) return;
      setState(() => _shut = false);
      _timer = Timer(_gap(), _blink);
    });
  }

  @override
  Widget build(BuildContext context) {
    return Stack(
      fit: StackFit.expand,
      children: [
        Image.asset(
          widget.base,
          fit: BoxFit.cover,
          filterQuality: FilterQuality.medium,
          gaplessPlayback: true,
          excludeFromSemantics: true,
          errorBuilder: (_, _, _) => widget.fallback,
        ),
        // Kept in the tree even when shut, so the render is decoded before it shows.
        if (widget.enabled)
          Opacity(
            opacity: _shut ? 1 : 0,
            child: Image.asset(
              widget.blink,
              fit: BoxFit.cover,
              filterQuality: FilterQuality.medium,
              gaplessPlayback: true,
              excludeFromSemantics: true,
              errorBuilder: (_, _, _) => const SizedBox.shrink(),
            ),
          ),
      ],
    );
  }
}
