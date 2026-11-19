import 'dart:async';

import 'package:flutter/material.dart' hide ConnectionState;
import 'package:excubia/src/rust/api/error.dart';
import 'package:excubia/src/rust/api/state.dart';
import 'package:excubia/src/rust/api/workspace.dart';

import '../core/api/connection_failure.dart';
import '../core/api/connection_text.dart';
import '../core/api/plans_api.dart';
import '../core/api/team_summary_api.dart';
import '../core/model/connection_state.dart';
import '../core/state/chat_controller.dart';
import '../core/state/hires_controller.dart';
import '../core/state/live_triage.dart';
import '../core/state/plans_controller.dart';
import '../core/state/recipes_controller.dart';
import '../core/state/schedules_controller.dart';
import '../core/state/team_summary_controller.dart';
import 'agent_detail_screen.dart';
import 'new_conversation_screen.dart';
import 'plans/plans_section.dart';
import 'schedules/schedules_section.dart';
import 'team/hire_actions.dart';
import 'team/hires_section.dart';
import 'team/team_panel_parts.dart';
import 'team/team_portrait_row.dart';
import 'team/team_summary_section.dart';
import 'team_projects.dart';
import 'theme.dart';
import 'widgets/agent_row.dart';
import 'widgets/empty_state.dart';
import 'widgets/message_panel.dart';
import 'widgets/status_dot.dart';

part 'agents_screen_actions.dart';
part 'agents_screen_shell.dart';

enum _TeamFilter { all, working, blocked }

/// The secondary "Equipo" tab: the live triage list with the same detail, stop
/// and new-conversation actions as APK4, plus the wf44 filters.
class AgentsScreen extends StatefulWidget {
  const AgentsScreen({
    super.key,
    required this.triage,
    required this.chat,
    required this.hires,
    required this.recipes,
    required this.summary,
    required this.plans,
    required this.schedules,
    required this.onUnpaired,
    this.onOpenChat,
  });

  final LiveTriage triage;
  final ChatController chat;
  final HiresController hires;
  final RecipesController recipes;
  final TeamSummaryController summary;
  final PlansController plans;
  final SchedulesController schedules;
  final Future<void> Function() onUnpaired;

  /// Opens the Chat tab; the empty team's way out when nobody can be contracted here.
  final VoidCallback? onOpenChat;

  @override
  State<AgentsScreen> createState() => _AgentsScreenState();
}

class _AgentsScreenState extends State<AgentsScreen>
    with WidgetsBindingObserver {
  final TextEditingController _search = TextEditingController();
  String? _pendingPane;
  Timer? _pendingTimer;
  _TeamFilter _filter = _TeamFilter.all;

  LiveTriage get _triage => widget.triage;
  ChatController get _chat => widget.chat;
  HiresController get _hires => widget.hires;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _triage.addListener(_onChanged);
    _chat.addListener(_onChanged);
    _hires.addListener(_onChanged);
    // The team's own reads decide the empty state too: a read that lands later must redo it.
    widget.summary.addListener(_onChanged);
    widget.plans.addListener(_onChanged);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _pendingTimer?.cancel();
    _search.dispose();
    _triage.removeListener(_onChanged);
    _chat.removeListener(_onChanged);
    _hires.removeListener(_onChanged);
    widget.summary.removeListener(_onChanged);
    widget.plans.removeListener(_onChanged);
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _triage.refreshNow();
      unawaited(_hires.refresh());
    }
  }

  void _onChanged() {
    if (!mounted) return;
    setState(() {});
    unawaited(_openPendingIfPresent());
  }

  @override
  Widget build(BuildContext context) {
    final connection = _triage.connection;
    final state = _triage.currentState;
    final visible = _visibleAgents(state);
    return _Shell(
      subtitle: _subtitleFor(connection, state, visible.length),
      onRefresh: () {
        unawaited(_hires.refresh());
        unawaited(_triage.refreshNow());
      },
      onNewConversation: () => _openNewConversation(),
      child: _bodyFor(connection, state, visible),
    );
  }

  List<AgentEntry> _visibleAgents(StateSnapshot? state) {
    final agents = state?.agents ?? const <AgentEntry>[];
    final filtering = _filtersShown(agents.length);
    final query = filtering ? _search.text.trim().toLowerCase() : '';
    return agents.where((agent) {
      if (!filtering) return true;
      final matchesFilter = switch (_filter) {
        _TeamFilter.all => true,
        _TeamFilter.working => agent.status == AgentStatus.working,
        _TeamFilter.blocked => agent.status == AgentStatus.blocked,
      };
      if (!matchesFilter) return false;
      if (query.isEmpty) return true;
      return agent.project.toLowerCase().contains(query) ||
          (agent.title ?? '').toLowerCase().contains(query) ||
          agent.agent.toLowerCase().contains(query);
    }).toList();
  }

  String _subtitleFor(
    ConnectionState connection,
    StateSnapshot? state,
    int visible,
  ) {
    final total = state?.agents.length ?? 0;
    final hired = _hires.hires.length;
    final count = hired > 0
        ? teamCountLabel(hired: hired, active: total)
        : visible == total
        ? _countLabel(total)
        : '$visible de ${_countLabel(total)}';
    return switch (connection) {
      Connecting() => 'Conectando con el equipo…',
      Live(:final updatedAt) => '$count · ${ageLabel(updatedAt)}',
      Offline(:final updatedAt, :final failure) =>
        '${connectionLabel(failure)} · último estado ${ageLabel(updatedAt)}',
      Failed() => 'Sin conexión con el equipo',
      AwaitingApproval() => 'Pendiente de aprobación',
    };
  }

  /// The live agents on the PC, not the people contracted; the header says so.
  String _countLabel(int count) =>
      count == 1 ? '1 agente abierto' : '$count agentes abiertos';

  /// One attempt, started by a tap: the loop is not restarted by a timer.
  Future<void> _reconnect() async {
    _triage.start();
    await _triage.refreshNow();
  }

  Widget _bodyFor(
    ConnectionState connection,
    StateSnapshot? state,
    List<AgentEntry> visible,
  ) {
    if (connection is AwaitingApproval) {
      return AwaitingApprovalPanel(onRetry: _reconnect);
    }
    if (connection is Failed) {
      final repair = connection.kind == FailureKind.repair;
      return MessagePanel(
        icon: repair ? Icons.link_off : Icons.error_outline,
        title: repair
            ? 'Este teléfono ya no está emparejado'
            : 'No se puede leer la lista',
        detail: connection.message,
        hint: connection.action,
        actionLabel: repair ? 'Volver a emparejar' : 'Reintentar',
        onAction: repair ? widget.onUnpaired : _reconnect,
      );
    }

    if (state == null || state.agents.isEmpty) {
      if (state == null && connection is Connecting) {
        return Center(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              SizedBox(
                width: 28,
                height: 28,
                child: CircularProgressIndicator(
                  strokeWidth: 2.5,
                  color: context.colors.accentText,
                ),
              ),
              const SizedBox(height: Spacing.md),
              Text('Conectando…', style: context.text.bodySecondary),
            ],
          ),
        );
      }
      return ListView(
        padding: const EdgeInsets.only(bottom: Spacing.xl),
        children: [
          // Offline with nobody on the team: the one empty state stays, the connection is one line.
          if (connection is Offline && _teamIsEmpty)
            _OfflineBanner(
              reason: connection.reason,
              updatedAt: connection.updatedAt,
              failure: connection.failure,
            ),
          ..._teamHead(),
          if (!_teamIsEmpty)
            TeamHiresSection(hires: _hires, recipes: widget.recipes),
          if (_hires.hires.isNotEmpty) _portraitRow(),
          ..._projectCards(),
          SchedulesSection(schedules: widget.schedules, oficioOf: _oficioOf),
          if (!_teamIsEmpty && !_onlyProjects) _emptyTeam(connection, state),
        ],
      );
    }

    final showBanner = connection is Offline;
    final rows = <Widget>[];
    if (showBanner) {
      rows.add(
        _OfflineBanner(
          reason: connection.reason,
          updatedAt: connection.updatedAt,
          failure: connection.failure,
        ),
      );
    }
    final projects = _chat.projects;
    final overview = groupTeam(projects: projects, agents: visible);
    final unfiltered =
        _search.text.trim().isEmpty && _filter == _TeamFilter.all;
    if (visible.isEmpty) {
      rows.add(
        Padding(
          padding: const EdgeInsets.all(Spacing.xl),
          child: Text(
            'Ningún agente coincide con el filtro.',
            style: context.text.bodySecondary,
            textAlign: TextAlign.center,
          ),
        ),
      );
    } else if (projects.isEmpty) {
      rows.addAll(
        visible.map(
          (agent) =>
              AgentRow(agent: agent, onTap: () => _openDetail(agent.paneId)),
        ),
      );
    } else {
      for (final group in overview.projects) {
        final hasVisibleAgent =
            group.leadAgent != null || group.specialists.isNotEmpty;
        if (!hasVisibleAgent && !unfiltered) continue;
        rows.add(TeamProjectGroupCard(group: group, onOpenAgent: _openDetail));
      }
      if (overview.unassigned.isNotEmpty) {
        rows.add(
          Padding(
            padding: const EdgeInsets.fromLTRB(
              Spacing.lg,
              Spacing.md,
              Spacing.lg,
              Spacing.xs,
            ),
            child: Text('SIN PROYECTO', style: context.text.label),
          ),
        );
        rows.addAll(
          overview.unassigned.map(
            (agent) =>
                AgentRow(agent: agent, onTap: () => _openDetail(agent.paneId)),
          ),
        );
      }
    }
    return ListView(
      padding: const EdgeInsets.only(bottom: Spacing.xl),
      children: [
        ..._teamHead(),
        if (_filtersShown(state.agents.length)) _header(),
        if (!_teamIsEmpty)
          TeamHiresSection(hires: _hires, recipes: widget.recipes),
        if (_hires.hires.isNotEmpty) _portraitRow(),
        ...rows,
        SchedulesSection(schedules: widget.schedules, oficioOf: _oficioOf),
      ],
    );
  }

  /// The name of an oficio for «Automatizar»; a role no longer in the catalog keeps a plain word.
  String _oficioOf(String roleId) => _hires.roleById(roleId)?.title ?? 'Oficio';

  /// Nobody contracted, nobody on the summary, no plan, no project card: nothing to list. It is
  /// said only once every read has answered or failed, so «vacío» never shows before the host has spoken.
  bool get _teamIsEmpty =>
      _hiresAnswered &&
      _hires.hires.isEmpty &&
      _summaryAnswered &&
      widget.summary.members.isEmpty &&
      _plansAnswered &&
      widget.plans.plans.isEmpty &&
      _projectsAnswered &&
      _chat.projects.isEmpty;

  /// Only project cards on the team: «PLANES PROPUESTOS» and «TU EQUIPO» would show nothing but
  /// their own empty states, so they stay away and the cards say the rest.
  bool get _onlyProjects =>
      _projectsAnswered &&
      _chat.projects.isNotEmpty &&
      _hiresAnswered &&
      _hires.hires.isEmpty &&
      _summaryAnswered &&
      widget.summary.members.isEmpty &&
      widget.summary.error == null &&
      _plansAnswered &&
      widget.plans.plans.isEmpty &&
      widget.plans.error == null &&
      !widget.plans.denied;

  bool get _hiresAnswered => _hires.loaded || _hires.failure != null;
  bool get _summaryAnswered =>
      widget.summary.loaded || widget.summary.error != null;
  bool get _plansAnswered => widget.plans.loaded || widget.plans.error != null;
  bool get _projectsAnswered => _chat.bootstrapAnswered;

  /// The search and the filter chips appear only when there is a list worth filtering (more than
  /// four open agents and project cards together) or a search or filter is already on, so it can be cleared.
  bool _filtersShown(int openAgents) {
    final applied =
        _search.text.trim().isNotEmpty || _filter != _TeamFilter.all;
    return applied || openAgents + _chat.projects.length > 4;
  }

  /// Plans and «Tu equipo»; with nobody on either, one invitation instead of the empty sections.
  List<Widget> _teamHead() {
    // Until the contracted people are read, nobody is known: no empty state, no jump.
    if (!_hiresAnswered) {
      return const [TeamLoadingBlock(label: 'Cargando tu equipo')];
    }
    if (_teamIsEmpty) {
      final notice = _teamNotice();
      return [
        ?notice,
        EmptyState(
          icon: Icons.groups_outlined,
          title: 'Tu equipo está vacío',
          detail: 'Pídele a Alfred que arme uno',
          // Without APPROVE the host refuses hires: the phone offers the chat instead.
          actionLabel: _hires.canHire ? 'Contratar' : 'Hablar con Alfred',
          onAction: _hires.canHire
              ? (_hires.busy ? null : () => openHireCatalog(context, _hires))
              : widget.onOpenChat,
        ),
      ];
    }
    if (_onlyProjects) return const [];
    return [
      PlansSection(plans: widget.plans),
      TeamSummarySection(
        summary: widget.summary,
        hires: _hires,
        recipes: widget.recipes,
        plans: widget.plans,
      ),
    ];
  }

  /// The one small notice over the empty state: the first read that failed or was refused.
  Widget? _teamNotice() {
    final hiresFailure = _hires.failure;
    if (hiresFailure != null) {
      return TeamNoticeBox(
        text: hiresFailure.message,
        onRetry: _hires.busy ? null : () => _hires.load(),
      );
    }
    final summaryError = widget.summary.error;
    if (summaryError != null) {
      return TeamNoticeBox(
        text: teamSummaryFailureText(summaryError),
        onRetry: teamSummaryWaitingText(summaryError) == null
            ? () => widget.summary.refresh()
            : null,
      );
    }
    final planError = widget.plans.error;
    if (planError != null) {
      return TeamNoticeBox(
        text: planReadFailureText(planError),
        onRetry: planReadWaitingText(planError) == null
            ? () => widget.plans.refresh()
            : null,
      );
    }
    if (widget.plans.denied) {
      return const TeamNoticeBox(text: planPermissionText);
    }
    return null;
  }

  /// The contracted people as portraits; a tap opens that person's sheet.
  Widget _portraitRow() => TeamPortraitRow(
    hires: _hires.hires,
    onOpen: (hire) => showHireActions(
      context,
      _hires,
      hire,
      widget.recipes,
      onHire: hireCatalogAction(context, _hires),
    ),
  );

  /// The project cards when no agent is open; the same cards as the list above.
  List<Widget> _projectCards() => [
    for (final group in groupTeam(
      projects: _chat.projects,
      agents: const <AgentEntry>[],
    ).projects)
      TeamProjectGroupCard(group: group, onOpenAgent: _openDetail),
  ];

  /// Nobody is listed. With people hired, "nobody is running" is the true state.
  /// Offline keeps the retry panel; online says it lightly, because «Contratar»
  /// is the screen's one primary action.
  Widget _emptyTeam(ConnectionState connection, StateSnapshot? state) {
    if (_hires.hires.isNotEmpty) {
      return Padding(
        padding: const EdgeInsets.all(Spacing.xl),
        child: Text(
          'Nadie trabajando ahora',
          style: context.text.bodySecondary,
          textAlign: TextAlign.center,
        ),
      );
    }
    if (connection is! Offline) {
      return EmptyState(
        icon: Icons.inbox_outlined,
        title: 'No hay agentes',
        detail: 'Cuando un agente se ejecute aparecerá aquí.',
        actionLabel: 'Nueva conversación',
        onAction: () => _openNewConversation(),
      );
    }
    final failure = connection.failure;
    return MessagePanel(
      icon: Icons.inbox_outlined,
      title: 'No hay agentes',
      detail: state == null
          ? failure == null
                ? 'Sin conexión: no hay un estado guardado que mostrar.'
                : connectionText(failure)
          : 'El último estado guardado no tenía agentes.',
      actionLabel: 'Nueva conversación',
      onAction: () => _openNewConversation(),
      secondaryLabel: 'Reintentar',
      onSecondary: _reconnect,
    );
  }

  Widget _header() {
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        Spacing.lg,
        Spacing.sm,
        Spacing.lg,
        Spacing.sm,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          TextField(
            controller: _search,
            onChanged: (_) => setState(() {}),
            style: context.text.body,
            decoration: const InputDecoration(
              hintText: 'Buscar agente o tarea…',
              prefixIcon: Icon(Icons.search),
              isDense: true,
            ),
          ),
          const SizedBox(height: Spacing.sm),
          Wrap(
            spacing: Spacing.sm,
            runSpacing: Spacing.sm,
            children: [
              for (final entry in const <_TeamFilter, String>{
                _TeamFilter.all: 'Todos',
                _TeamFilter.working: 'Trabajando',
                _TeamFilter.blocked: 'Esperando',
              }.entries)
                ChoiceChip(
                  label: Text(entry.value),
                  selected: _filter == entry.key,
                  onSelected: (_) => setState(() => _filter = entry.key),
                  labelStyle: context.text.small,
                  selectedColor: context.colors.accentSoft,
                  backgroundColor: context.colors.raised,
                  side: BorderSide(color: context.colors.outlineSoft),
                  padding: const EdgeInsets.symmetric(
                    horizontal: Spacing.sm,
                    vertical: Spacing.md,
                  ),
                ),
            ],
          ),
        ],
      ),
    );
  }
}
