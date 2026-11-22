import 'package:flutter/material.dart';
import 'package:excubia/src/rust/api/chat/model_team.dart';
import 'package:excubia/src/rust/api/state.dart';

import 'theme.dart';
import 'widgets/agent_avatar.dart';
import 'widgets/agent_text.dart';
import 'widgets/comic/comic_slab.dart';
import 'widgets/comic/comic_tag.dart';

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

/// A project as a cut comic slab; its agents hang below as narrower strips. Open by
/// default; the slab's tap folds them, and the chevron turns with the fold.
class TeamProjectGroupCard extends StatefulWidget {
  const TeamProjectGroupCard({
    super.key,
    required this.group,
    required this.onOpenAgent,
  });

  final TeamProjectGroup group;
  final ValueChanged<String> onOpenAgent;

  @override
  State<TeamProjectGroupCard> createState() => _TeamProjectGroupCardState();
}

class _TeamProjectGroupCardState extends State<TeamProjectGroupCard> {
  bool _folded = false;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final group = widget.group;
    final lead = group.project.lead;
    // One agent needs no count; nobody needs none, the line below says so.
    final count = switch (group.seats) {
      0 || 1 => null,
      _ => '${group.seats} agentes',
    };
    final folder = _folderLine(group.project);
    final idleLead = _idleLead(group);
    final rows = <Widget>[
      if (lead != null && !idleLead)
        _MemberRow(
          name: lead.name,
          // The badge tells the lead apart only when someone works under them.
          role: group.specialists.isEmpty ? null : 'Responsable',
          status: group.leadAgent?.status ?? _seatStatus(lead.status),
          detail: group.leadAgent?.title,
          onTap: (group.leadAgent != null || lead.paneId != null)
              ? () =>
                    widget.onOpenAgent(group.leadAgent?.paneId ?? lead.paneId!)
              : null,
        ),
      for (final agent in group.specialists)
        _MemberRow(
          name: agentNameOf(agent),
          role: 'Especialista',
          status: agent.status,
          detail: agent.title,
          onTap: () => widget.onOpenAgent(agent.paneId),
        ),
    ];
    final hasRows = rows.isNotEmpty;
    final open = hasRows && !_folded;
    final motion = !MediaQuery.disableAnimationsOf(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        Spacing.lg,
        Spacing.xs,
        Spacing.lg,
        Spacing.sm,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Semantics(
            button: hasRows,
            expanded: hasRows ? open : null,
            label: [group.project.name, ?count].join(', '),
            excludeSemantics: true,
            child: GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: hasRows ? () => setState(() => _folded = !_folded) : null,
              child: ConstrainedBox(
                constraints: const BoxConstraints(minHeight: 48),
                child: Center(
                  child: ComicSlab(
                    color: c.deep,
                    cut: true,
                    child: Row(
                      children: [
                        Icon(Icons.folder_outlined, size: 20, color: c.onDeep),
                        const SizedBox(width: Spacing.sm),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Text(
                                group.project.name,
                                style: context.text.title.copyWith(
                                  color: c.onDeep,
                                  fontWeight: FontWeight.w800,
                                ),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                              ),
                              if (folder != null)
                                Text(
                                  folder,
                                  style: context.text.small.copyWith(
                                    color: c.onDeep,
                                  ),
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                ),
                            ],
                          ),
                        ),
                        if (count != null) ...[
                          const SizedBox(width: Spacing.sm),
                          Flexible(
                            child: FittedBox(
                              fit: BoxFit.scaleDown,
                              child: ComicTag(label: count, caps: false),
                            ),
                          ),
                        ],
                        if (hasRows) ...[
                          const SizedBox(width: Spacing.xs),
                          AnimatedRotation(
                            turns: open ? 0.25 : 0,
                            duration: motion
                                ? const Duration(milliseconds: 180)
                                : Duration.zero,
                            child: Icon(
                              Icons.chevron_right,
                              size: 22,
                              color: c.onDeep,
                            ),
                          ),
                        ],
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
          if (open)
            Padding(
              padding: const EdgeInsets.only(left: Spacing.md, top: Spacing.sm),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  for (final row in rows)
                    Padding(
                      padding: const EdgeInsets.only(bottom: Spacing.sm),
                      child: row,
                    ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

/// One agent in a project: a framed portrait, the name, a role stamp and a status dot.
/// The dot is a colour only; its words are in the row's subtitle and its label.
class _MemberRow extends StatelessWidget {
  const _MemberRow({
    required this.name,
    required this.role,
    required this.status,
    required this.detail,
    required this.onTap,
  });

  final String name;
  final String? role;
  final AgentStatus status;
  final String? detail;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final shown = stripRoleSuffix(name);
    final statusText = c.statusLabel(status);
    final subtitle = agentSubtitle(status, statusText, detail);
    return Semantics(
      button: onTap != null,
      label: [shown, ?role, ?subtitle].join(', '),
      excludeSemantics: true,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: ComicSlab(
          color: c.pale,
          shadow: 2,
          padding: const EdgeInsets.symmetric(
            horizontal: Spacing.sm,
            vertical: Spacing.sm,
          ),
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 48),
            child: Row(
              children: [
                AgentAvatar(name: name, status: status, size: 44, framed: true),
                const SizedBox(width: Spacing.md),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      Text(
                        shown,
                        style: context.text.body.copyWith(
                          color: c.onPale,
                          fontWeight: FontWeight.w700,
                        ),
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                      ),
                      if (subtitle != null)
                        Text(
                          subtitle,
                          style: context.text.small.copyWith(color: c.onPale),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                    ],
                  ),
                ),
                if (role != null) ...[
                  const SizedBox(width: Spacing.xs),
                  Flexible(
                    child: FittedBox(
                      fit: BoxFit.scaleDown,
                      child: ComicTag(
                        label: role!,
                        caps: false,
                        tone: role == 'Responsable'
                            ? ComicTagTone.hot
                            : ComicTagTone.deep,
                      ),
                    ),
                  ),
                ],
                const SizedBox(width: Spacing.xs),
                Container(
                  width: 10,
                  height: 10,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    color: _statusInk(c, status),
                  ),
                ),
                if (onTap != null) ...[
                  const SizedBox(width: Spacing.xs),
                  Icon(Icons.chevron_right, size: 20, color: c.onPale),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

Color _statusInk(HerdrColors c, AgentStatus status) => switch (status) {
  AgentStatus.working => c.statusWorking,
  AgentStatus.blocked => c.statusBlocked,
  AgentStatus.done => c.statusDone,
  _ => c.statusIdle,
};

/// The folder name under the project title. Hidden when it says the title again
/// («Proyecto web» / «proyecto-web») or is a slug («bench-proj»): an id, not a name.
String? _folderLine(ChatProject project) {
  final folder = project.rootLabel.trim();
  if (folder.isEmpty || folder.contains('-') || folder.contains('_')) {
    return null;
  }
  return _words(folder) == _words(project.name) ? null : folder;
}

String _words(String text) =>
    text.toLowerCase().replaceAll(RegExp(r'[\s_-]+'), ' ').trim();

String? _workspaceOf(String? paneId) {
  if (paneId == null) return null;
  final separator = paneId.indexOf(':');
  if (separator <= 0) return null;
  return paneId.substring(0, separator);
}

/// The lead of a project with nobody under them and no live agent, whose status
/// is unknown, idle or done: a bare name would say nothing, so the card omits it.
/// A blocked or working lead stays visible.
bool _idleLead(TeamProjectGroup group) {
  final lead = group.project.lead;
  if (lead == null || group.specialists.isNotEmpty || group.leadAgent != null) {
    return false;
  }
  final status = _seatStatus(lead.status);
  return status == AgentStatus.unknown ||
      status == AgentStatus.idle ||
      status == AgentStatus.done;
}

AgentStatus _seatStatus(String status) => switch (status) {
  'working' => AgentStatus.working,
  'blocked' => AgentStatus.blocked,
  'done' => AgentStatus.done,
  'idle' => AgentStatus.idle,
  _ => AgentStatus.unknown,
};
