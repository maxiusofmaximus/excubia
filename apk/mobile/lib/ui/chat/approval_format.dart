import 'package:herdr_notify/src/rust/api/chat/model_team.dart';

/// Seconds until the host's deadline, or null when the host sent none. The
/// deadline is the host's `expires_at`, never the local TTL.
int? secondsLeft(String? expiresAt, DateTime now) {
  final deadline = expiresAt == null ? null : DateTime.tryParse(expiresAt);
  if (deadline == null) return null;
  return deadline.difference(now).inSeconds;
}

/// `m:ss`, never negative.
String formatCountdown(int seconds) {
  final shown = seconds < 0 ? 0 : seconds;
  final minutes = shown ~/ 60;
  final rest = (shown % 60).toString().padLeft(2, '0');
  return '$minutes:$rest';
}

/// The last two folders of a path, as a short label: `…/demo/api`.
String? shortCwd(String? cwd) {
  if (cwd == null || cwd.trim().isEmpty) return null;
  final parts = cwd
      .split(RegExp(r'[\\/]'))
      .where((part) => part.isNotEmpty)
      .toList();
  if (parts.isEmpty) return null;
  if (parts.length == 1) return parts.single;
  final tail = parts.sublist(parts.length - 2).join('/');
  return parts.length > 2 ? '…/$tail' : tail;
}

String? riskLabel(String? risk) => switch (risk) {
  null || '' => null,
  'low' => 'riesgo bajo',
  'medium' => 'riesgo medio',
  'high' => 'riesgo alto',
  final other => 'riesgo $other',
};

/// Who holds the approval above the user: `user` is the chip «te lo pasa Alfred».
String? levelLabel(String? level) => switch (level) {
  'user' => 'te lo pasa Alfred',
  'cto' => 'lo decide Alfred',
  'lead' => 'lo decide el lead',
  _ => null,
};

/// The lead seat that asked, found in the team. Specialists are not in the
/// chat bootstrap, so they resolve to null and the card says «Un agente».
ChatSeat? seatFor(String? seatId, List<ChatProject> projects) {
  if (seatId == null) return null;
  for (final project in projects) {
    final lead = project.lead;
    if (lead != null && lead.agentId == seatId) return lead;
  }
  return null;
}

/// The project whose lead asked, by name; null when unknown.
String? projectNameFor(String? seatId, List<ChatProject> projects) {
  if (seatId == null) return null;
  for (final project in projects) {
    final lead = project.lead;
    if (lead != null && lead.agentId == seatId) return project.name;
  }
  return null;
}

/// «Bob necesita permiso», or «Un agente» when the seat has no name.
String agentTitle(String? seatId, List<ChatProject> projects) {
  final name = seatFor(seatId, projects)?.name.trim() ?? '';
  return name.isEmpty ? 'Un agente' : name;
}
