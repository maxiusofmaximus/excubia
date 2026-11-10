import 'package:flutter/material.dart';
import 'package:excubia/src/rust/api/state.dart';

import '../theme.dart';

/// The status glyph, sized by urgency: only the blocked agent grows, so a single
/// thing pulls the eye. Each state has its own shape, not just its own colour.
class StatusDot extends StatelessWidget {
  const StatusDot({super.key, required this.status});

  final AgentStatus status;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Icon(
      c.iconForStatus(status),
      size: status == AgentStatus.blocked ? 20 : 18,
      color: c.forStatus(status),
    );
  }
}

/// The status word as a tag. Blocked is the one filled tag (coral fill, dark
/// text); the other states are plain coloured text.
class StatusChip extends StatelessWidget {
  const StatusChip({super.key, required this.status});

  final AgentStatus status;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final word = c.statusLabel(status);
    if (status != AgentStatus.blocked) {
      return Text(
        word,
        style: context.text.label.copyWith(color: c.labelForStatus(status)),
      );
    }
    return DecoratedBox(
      decoration: BoxDecoration(
        color: c.statusBlocked,
        borderRadius: BorderRadius.circular(999),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: Spacing.sm,
          vertical: 2,
        ),
        child: Text(
          word,
          style: context.text.label.copyWith(color: c.onAttention),
        ),
      ),
    );
  }
}

/// Relative age for "actualizado hace X"; the exact clock is not the point.
String ageLabel(DateTime? at, {DateTime? now}) {
  if (at == null) return 'sin datos';
  final seconds = (now ?? DateTime.now()).difference(at).inSeconds;
  if (seconds < 5) return 'ahora mismo';
  if (seconds < 60) return 'hace $seconds s';
  final minutes = seconds ~/ 60;
  if (minutes < 60) return 'hace $minutes min';
  final hours = minutes ~/ 60;
  if (hours < 24) return 'hace $hours h';
  return 'hace ${hours ~/ 24} d';
}
