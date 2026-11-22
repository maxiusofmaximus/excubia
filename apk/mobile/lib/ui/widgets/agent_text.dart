import 'package:excubia/src/rust/api/state.dart';

import '../../core/app_info.dart';
import 'agent_avatar.dart';
import 'status_dot.dart';

/// «Terminó hace 2 h» for a finished agent whose last movement the PC knows;
/// null otherwise, so a live agent or an old host shows nothing new.
String? agentEndedText(AgentEntry agent, {DateTime? now}) {
  final ms = agent.lastActiveMs;
  if (!agent.restored || ms == null) return null;
  // toInt(): PlatformInt64 is BigInt on web and int on native.
  final at = DateTime.fromMillisecondsSinceEpoch(ms.toInt());
  return 'Terminó ${ageLabel(at, now: now)}';
}

/// The host's role tag at the end of a pane title («Ernesto — lead»).
final _roleTag = RegExp(
  r'\s+[—-]\s+(lead|specialist|especialista)$',
  caseSensitive: false,
);

/// «Ernesto — lead» → «Ernesto». Only for text the screen shows; the name the
/// portrait is found by is not passed through here.
String stripRoleSuffix(String text) => text.replaceFirst(_roleTag, '');

/// Whether the PC's project field names a real project. For an agent with none
/// the PC fills it with its own label («core» by default, the app's name in
/// older data), which is not a project.
bool agentHasProject(AgentEntry agent) {
  final project = agent.project.trim().toLowerCase();
  return project.isNotEmpty &&
      project != appName.toLowerCase() &&
      project != 'core';
}

/// The project a row shows; without a real project, the agent's character.
String agentProjectLabel(AgentEntry agent) =>
    agentHasProject(agent) ? agent.project : agentNameOf(agent);

/// The line under an agent's name: the state word and the task. An unknown state
/// never reads «Sin actividad reciente»: it shows the task alone, or nothing. A pane title
/// that is only a character's name («Ernesto — lead») is not a task and is left out.
String? agentSubtitle(AgentStatus status, String label, String? task) {
  var text = stripRoleSuffix((task ?? '').trim()).trim();
  if (agentProfileFor(text) != null) text = '';
  if (status == AgentStatus.unknown) return text.isEmpty ? null : text;
  return text.isEmpty ? label : '$label · $text';
}
