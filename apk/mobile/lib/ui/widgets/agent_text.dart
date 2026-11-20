import 'package:excubia/src/rust/api/state.dart';

import 'agent_avatar.dart';

/// The host's role tag at the end of a pane title («Ernesto — lead»).
final _roleTag = RegExp(
  r'\s+[—-]\s+(lead|specialist|especialista)$',
  caseSensitive: false,
);

/// «Ernesto — lead» → «Ernesto». Only for text the screen shows; the name the
/// portrait is found by is not passed through here.
String stripRoleSuffix(String text) => text.replaceFirst(_roleTag, '');

/// The line under an agent's name: the state word and the task. An unknown state
/// never reads «Sin actividad reciente»: it shows the task alone, or nothing. A pane title
/// that is only a character's name («Ernesto — lead») is not a task and is left out.
String? agentSubtitle(AgentStatus status, String label, String? task) {
  var text = stripRoleSuffix((task ?? '').trim()).trim();
  if (agentProfileFor(text) != null) text = '';
  if (status == AgentStatus.unknown) return text.isEmpty ? null : text;
  return text.isEmpty ? label : '$label · $text';
}
