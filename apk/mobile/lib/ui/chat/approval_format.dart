import 'package:excubia/src/rust/api/chat/model_team.dart';

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
  'lead' => 'lo decide el responsable del proyecto',
  _ => null,
};

/// Where a mail of the batch goes, in the card's words.
String destLabel(String dest) => switch (dest) {
  'junkemail' => 'No deseado',
  'deleteditems' => 'Eliminados',
  'inbox' => 'Bandeja de entrada',
  _ => 'Destino desconocido',
};

/// The triage verdict of one mail. Clean means no automatic check found anything,
/// never that the mail is safe.
String? triageLabel(String? level) => switch (level) {
  'phish_candidate' => 'posible phishing',
  'suspicious' => 'sospechoso',
  'bulk' => 'envío masivo',
  'unknown' => 'sin datos de autenticación',
  'clean' => 'sin señales automáticas',
  _ => null,
};

/// The seat that asked: Alfred, or a lead found in the team. Specialists are not
/// in the chat bootstrap, so they resolve to null and the card says «Un agente».
ChatSeat? seatFor(
  String? seatId,
  List<ChatProject> projects,
  ChatSeat? alfred,
) {
  if (seatId == null) return null;
  if (alfred != null && alfred.agentId == seatId) return alfred;
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
String agentTitle(
  String? seatId,
  List<ChatProject> projects,
  ChatSeat? alfred, {
  String tool = '',
}) {
  // The host places a mail action on Alfred's behalf, with no seat of its own.
  final asked = seatId == null && tool.startsWith('mail.')
      ? alfred?.agentId
      : seatId;
  final name = seatFor(asked, projects, alfred)?.name.trim() ?? '';
  return name.isEmpty ? 'Un agente' : name;
}

const _writeTools = {'write', 'edit', 'multiedit', 'notebookedit'};
const _shellTools = {'bash', 'shell', 'powershell', 'pwsh', 'cmd'};
final RegExp _deleteCommand = RegExp(
  r'(?:^|[;&|(]\s*)(?:sudo\s+)?(?:rm|del|erase|rmdir|remove-item|unlink)\s',
  caseSensitive: false,
);

/// A shell tool: its command can change or delete files.
bool isShellTool(String? tool) =>
    _shellTools.contains((tool ?? '').toLowerCase());

/// What the agent wants to do, in words the operator reads. Null when the tool
/// says nothing clear; the card then says «necesita permiso».
String? actionPhrase(String? tool, String? command) {
  final name = (tool ?? '').toLowerCase();
  if (_writeTools.contains(name)) return 'guardar un archivo';
  if (_shellTools.contains(name)) {
    return _deleteCommand.hasMatch(command ?? '')
        ? 'borrar archivos'
        : 'ejecutar un comando';
  }
  if (name == 'read') return 'leer un archivo';
  return null;
}

/// A timeline line in the operator's words: «Aprobado», «Esperando tu OK», with
/// the step after it unless the step is only a tool's name («edit Write»).
String timelineLineText(String summary) {
  final text = summary.trim();
  final match = _timelineHead.firstMatch(text);
  if (match == null) return text;
  final head = switch (match.group(1)!.toLowerCase()) {
    'operador aprobó' => 'Aprobado',
    'operador denegó' => 'Denegado',
    'host expiró' => 'Caducó sin respuesta',
    _ => 'Esperando tu OK',
  };
  final step = match.group(2)!.trim();
  return step.isEmpty || _toolOnly.hasMatch(step) ? head : '$head · $step';
}

final RegExp _timelineHead = RegExp(
  r'^(operador aprobó|operador denegó|pendiente|host expiró):\s*(.*)$',
  caseSensitive: false,
);
final RegExp _toolOnly = RegExp(
  r'^(?:command|tool|edit|write|read|multiedit|notebookedit|bash|shell|powershell|pwsh|cmd|websearch|webfetch)\s+\S+$',
  caseSensitive: false,
);
