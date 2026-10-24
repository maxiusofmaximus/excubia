import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/state.dart';

import '../theme.dart';

/// The status dot, sized by urgency: only the blocked agent grows, so a single
/// thing pulls the eye.
class StatusDot extends StatelessWidget {
  const StatusDot({super.key, required this.status});

  final AgentStatus status;

  @override
  Widget build(BuildContext context) {
    final size = status == AgentStatus.blocked ? 12.0 : 10.0;
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        color: context.colors.forStatus(status),
        shape: BoxShape.circle,
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
