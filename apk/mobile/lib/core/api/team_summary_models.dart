import 'json_read.dart';

// «Tu equipo»: one line per contracted person and what they are doing (contract `api.md`, F1).

/// The five states the host names. The host decides them; the screen never reads one from
/// the headline.
enum TeamStatus { waitingForYou, blocked, working, done, idle }

/// A state the host does not name reads as idle, the same as the desktop.
TeamStatus teamStatusOf(String wire) => switch (wire) {
  'waiting_for_you' => TeamStatus.waitingForYou,
  'blocked' => TeamStatus.blocked,
  'working' => TeamStatus.working,
  'done' => TeamStatus.done,
  _ => TeamStatus.idle,
};

class TeamMember {
  const TeamMember({
    required this.agentId,
    required this.name,
    required this.roleTitle,
    required this.status,
    required this.headline,
    required this.since,
  });

  factory TeamMember.fromJson(Map<String, Object?> json) => TeamMember(
    agentId: jsonText(json, 'agent_id'),
    name: jsonText(json, 'name'),
    roleTitle: jsonText(json, 'role_title'),
    status: teamStatusOf(jsonText(json, 'status')),
    headline: jsonText(json, 'headline'),
    since: DateTime.tryParse(jsonOptionalText(json, 'since') ?? ''),
  );

  final String agentId;
  final String name;
  final String roleTitle;
  final TeamStatus status;

  /// The host's own sentence, shown as it came.
  final String headline;

  /// When the state began. Null when the host sent no readable time: the card then has no age.
  final DateTime? since;
}

/// The members in the host's order. The host already puts those waiting for the user first,
/// so the screen does not sort them again.
List<TeamMember> teamMembersFrom(Map<String, Object?> envelope) => [
  for (final item in jsonList(envelope, 'members'))
    TeamMember.fromJson(jsonObject(item)),
];
