import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';
import 'package:herdr_notify/src/rust/api/state.dart';

import 'theme.dart';
import 'widgets/status_dot.dart';

/// One project as the team surface sees it: its bootstrap lead plus the live
/// specialists found in `/v1/state`.
class TeamProjectGroup {
  const TeamProjectGroup({
    required this.project,
    required this.leadAgent,
    required this.specialists,
  });

  final ChatProject project;
  final AgentEntry? leadAgent;
  final List<AgentEntry> specialists;

  int get seats => (project.lead == null ? 0 : 1) + specialists.length;
}

class TeamOverview {
  const TeamOverview({required this.projects, required this.unassigned});

  final List<TeamProjectGroup> projects;
  final List<AgentEntry> unassigned;

  bool get isEmpty => projects.isEmpty && unassigned.isEmpty;
}

/// Group the live agents under the projects from `/v1/chat/bootstrap`. Agents
/// whose workspace does not match a project stay in "Sin proyecto" instead of
/// disappearing. The contract has no specialist names, so the CLI id is the
/// visible fallback and the task is the subtitle.
TeamOverview groupTeam({
  required List<ChatProject> projects,
  required List<AgentEntry> agents,
}) {
  final used = <String>{};
  final groups = <TeamProjectGroup>[];
  for (final project in projects) {
    final names = {
      project.name.trim().toLowerCase(),
      project.rootLabel.trim().toLowerCase(),
    }..remove('');
    final leadPane = project.lead?.paneId;
    final leadWorkspace = _workspaceOf(leadPane);
    final projectAgents = agents.where((agent) {
      if (used.contains(agent.paneId)) return false;
      if (leadPane != null && agent.paneId == leadPane) return true;
      // `/v1/state` reports the CLI workspace, not the chat project. Panels
      // spawned for one project share the lead's workspace prefix.
      if (leadWorkspace != null &&
          _workspaceOf(agent.paneId) == leadWorkspace) {
        return true;
      }
      return names.contains(agent.project.trim().toLowerCase());
    }).toList();
    AgentEntry? leadAgent;
    for (final agent in projectAgents) {
      if ((leadPane != null && agent.paneId == leadPane) ||
          (leadPane == null && agent.paneId == project.lead?.paneId)) {
        leadAgent = agent;
        break;
      }
    }
    final specialists = projectAgents
        .where((agent) => agent.paneId != leadAgent?.paneId)
        .toList();
    used.addAll(projectAgents.map((agent) => agent.paneId));
    groups.add(
      TeamProjectGroup(
        project: project,
        leadAgent: leadAgent,
        specialists: specialists,
      ),
    );
  }
  final unassigned = agents
      .where((agent) => !used.contains(agent.paneId))
      .toList();
  return TeamOverview(projects: groups, unassigned: unassigned);
}

class TeamProjectGroupCard extends StatelessWidget {
  const TeamProjectGroupCard({
    super.key,
    required this.group,
    required this.onOpenAgent,
  });

  final TeamProjectGroup group;
  final ValueChanged<String> onOpenAgent;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final lead = group.project.lead;
    final count = switch (group.seats) {
      1 => '1 asiento',
      _ => '${group.seats} asientos',
    };
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        Spacing.lg,
        Spacing.xs,
        Spacing.lg,
        Spacing.sm,
      ),
      child: Container(
        decoration: BoxDecoration(
          color: c.surface,
          borderRadius: BorderRadius.circular(14),
          border: Border.all(color: c.outlineSoft),
        ),
        clipBehavior: Clip.antiAlias,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Container(
              padding: const EdgeInsets.all(Spacing.md),
              color: c.raised,
              child: Row(
                children: [
                  Icon(Icons.folder_outlined, size: 20, color: c.accent),
                  const SizedBox(width: Spacing.sm),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          group.project.name,
                          style: context.text.title,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                        if (group.project.rootLabel.isNotEmpty &&
                            group.project.rootLabel != group.project.name)
                          Text(
                            group.project.rootLabel,
                            style: context.text.small,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                          ),
                      ],
                    ),
                  ),
                  Text(count, style: context.text.small),
                ],
              ),
            ),
            if (lead != null)
              _MemberRow(
                name: lead.name,
                role: 'Lead',
                status: group.leadAgent?.status ?? _seatStatus(lead.status),
                detail: group.leadAgent?.title,
                onTap: (group.leadAgent != null || lead.paneId != null)
                    ? () => onOpenAgent(group.leadAgent?.paneId ?? lead.paneId!)
                    : null,
              ),
            for (final agent in group.specialists)
              _MemberRow(
                name: _agentName(agent),
                role: 'Especialista',
                status: agent.status,
                detail: agent.title,
                onTap: () => onOpenAgent(agent.paneId),
              ),
            if (lead == null && group.specialists.isEmpty)
              Padding(
                padding: const EdgeInsets.all(Spacing.md),
                child: Text(
                  'Sin asientos vivos en este proyecto.',
                  style: context.text.bodySecondary,
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _MemberRow extends StatelessWidget {
  const _MemberRow({
    required this.name,
    required this.role,
    required this.status,
    required this.detail,
    required this.onTap,
  });

  final String name;
  final String role;
  final AgentStatus status;
  final String? detail;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final subtitle = detail == null || detail!.trim().isEmpty
        ? c.statusLabel(status)
        : '${c.statusLabel(status)} · ${detail!.trim()}';
    return Semantics(
      button: onTap != null,
      label: '$name, $role, $subtitle',
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: onTap,
          child: Container(
            constraints: const BoxConstraints(minHeight: 56),
            padding: const EdgeInsets.symmetric(
              horizontal: Spacing.md,
              vertical: Spacing.sm,
            ),
            decoration: BoxDecoration(
              border: Border(top: BorderSide(color: c.outlineSoft)),
            ),
            child: Row(
              children: [
                StatusDot(status: status),
                const SizedBox(width: Spacing.md),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      Text(
                        name,
                        style: context.text.body,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      const SizedBox(height: 2),
                      Text(
                        subtitle,
                        style: context.text.small,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: Spacing.sm),
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: Spacing.sm,
                    vertical: 2,
                  ),
                  decoration: BoxDecoration(
                    color: role == 'Lead' ? c.accentSoft : c.raised,
                    borderRadius: BorderRadius.circular(999),
                  ),
                  child: Text(
                    role,
                    style: context.text.label.copyWith(
                      color: role == 'Lead' ? c.accent : c.textSecondary,
                      fontSize: 10,
                    ),
                  ),
                ),
                if (onTap != null) ...[
                  const SizedBox(width: Spacing.xs),
                  Icon(Icons.chevron_right, size: 18, color: c.textMuted),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

String? _workspaceOf(String? paneId) {
  if (paneId == null) return null;
  final separator = paneId.indexOf(':');
  if (separator <= 0) return null;
  return paneId.substring(0, separator);
}

AgentStatus _seatStatus(String status) => switch (status) {
  'working' => AgentStatus.working,
  'blocked' => AgentStatus.blocked,
  'done' => AgentStatus.done,
  'idle' => AgentStatus.idle,
  _ => AgentStatus.unknown,
};

String _agentName(AgentEntry agent) {
  // `/v1/state` has no specialist name field. The pane title carries the seat
  // label in the middle segment ("desconocido · Ana · especialista"), so use it
  // before falling back to the CLI id.
  final segments = (agent.title ?? '')
      .split('·')
      .map((segment) => segment.trim())
      .where((segment) => segment.isNotEmpty)
      .toList();
  if (segments.length >= 2) {
    final candidate = segments[1];
    final normalized = candidate.toLowerCase();
    if (normalized != 'desconocido' && normalized != 'unknown') {
      return candidate;
    }
  }
  final raw = agent.agent.trim();
  if (raw.isEmpty ||
      raw.toLowerCase() == 'unknown' ||
      raw.toLowerCase() == 'desconocido') {
    return 'Especialista';
  }
  return raw[0].toUpperCase() + raw.substring(1);
}
