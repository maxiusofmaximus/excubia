import 'dart:async';

import 'package:flutter/material.dart' hide ConnectionState;
import 'package:herdr_notify/src/rust/api/error.dart';
import 'package:herdr_notify/src/rust/api/state.dart';
import 'package:herdr_notify/src/rust/api/workspace.dart';

import '../core/model/connection_state.dart';
import '../core/state/chat_controller.dart';
import '../core/state/live_triage.dart';
import 'agent_detail_screen.dart';
import 'new_conversation_screen.dart';
import 'team_projects.dart';
import 'theme.dart';
import 'widgets/agent_row.dart';
import 'widgets/status_dot.dart';

enum _TeamFilter { all, working, blocked }

/// The secondary "Equipo" tab: the live triage list with the same detail, stop
/// and new-conversation actions as APK4, plus the wf44 filters.
class AgentsScreen extends StatefulWidget {
  const AgentsScreen({
    super.key,
    required this.triage,
    required this.chat,
    required this.onUnpaired,
  });

  final LiveTriage triage;
  final ChatController chat;
  final Future<void> Function() onUnpaired;

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

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _triage.addListener(_onChanged);
    _chat.addListener(_onChanged);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _pendingTimer?.cancel();
    _search.dispose();
    _triage.removeListener(_onChanged);
    _chat.removeListener(_onChanged);
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _triage.refreshNow();
    }
  }

  void _onChanged() {
    if (!mounted) return;
    setState(() {});
    unawaited(_openPendingIfPresent());
  }

  Future<void> _openNewConversation({String? cwd}) async {
    final created = await Navigator.of(context).push<CreatedWorkspace>(
      MaterialPageRoute<CreatedWorkspace>(
        builder: (_) => NewConversationScreen(triage: _triage, initialCwd: cwd),
      ),
    );
    if (created == null || !mounted) return;
    _awaitPane(created.paneId);
  }

  void _awaitPane(String paneId) {
    _pendingPane = paneId;
    _pendingTimer?.cancel();
    _pendingTimer = Timer(const Duration(seconds: 8), () {
      if (!mounted || _pendingPane == null) return;
      _pendingPane = null;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text(
            'La conversación se creó; aparecerá en la lista al conectarse.',
          ),
        ),
      );
    });
    unawaited(_openPendingIfPresent());
  }

  Future<void> _openPendingIfPresent() async {
    final paneId = _pendingPane;
    if (paneId == null) return;
    final state = _triage.currentState;
    if (state == null || !state.agents.any((agent) => agent.paneId == paneId)) {
      return;
    }
    _pendingPane = null;
    _pendingTimer?.cancel();
    await _openDetail(paneId);
  }

  Future<void> _openDetail(String paneId) async {
    final result = await Navigator.of(context).push<AgentDetailResult>(
      MaterialPageRoute<AgentDetailResult>(
        builder: (_) => AgentDetailScreen(triage: _triage, paneId: paneId),
      ),
    );
    if (!mounted || result == null) return;
    switch (result) {
      case AgentStopped(:final alreadyGone):
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              alreadyGone
                  ? 'El agente ya había terminado.'
                  : 'Agente detenido.',
            ),
          ),
        );
      case ConversationCreated(:final created):
        _awaitPane(created.paneId);
    }
  }

  @override
  Widget build(BuildContext context) {
    final connection = _triage.connection;
    final state = _triage.currentState;
    final visible = _visibleAgents(state);
    return _Shell(
      subtitle: _subtitleFor(connection, state, visible.length),
      onRefresh: () => _triage.refreshNow(),
      onNewConversation: () => _openNewConversation(),
      child: _bodyFor(connection, state, visible),
    );
  }

  List<AgentEntry> _visibleAgents(StateSnapshot? state) {
    final agents = state?.agents ?? const <AgentEntry>[];
    final query = _search.text.trim().toLowerCase();
    return agents.where((agent) {
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
    final count = visible == total
        ? _countLabel(total)
        : '$visible de ${_countLabel(total)}';
    return switch (connection) {
      Connecting() => 'Conectando con el equipo…',
      Live(:final updatedAt) => '$count · ${ageLabel(updatedAt)}',
      Offline(:final updatedAt) =>
        'Sin conexión · último estado ${ageLabel(updatedAt)}',
      Failed() => 'Sin conexión con el equipo',
    };
  }

  String _countLabel(int count) => count == 1 ? '1 agente' : '$count agentes';

  Widget _bodyFor(
    ConnectionState connection,
    StateSnapshot? state,
    List<AgentEntry> visible,
  ) {
    if (connection is Failed) {
      final repair = connection.kind == FailureKind.repair;
      return _MessagePanel(
        icon: repair ? Icons.link_off : Icons.error_outline,
        title: repair
            ? 'Este teléfono ya no está emparejado'
            : 'No se puede leer la lista',
        detail: connection.message,
        hint: connection.action,
        actionLabel: repair ? 'Volver a emparejar' : 'Reintentar',
        onAction: repair
            ? widget.onUnpaired
            : () async {
                _triage.start();
                await _triage.refreshNow();
              },
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
                  color: context.colors.accent,
                ),
              ),
              const SizedBox(height: Spacing.md),
              Text('Conectando…', style: context.text.bodySecondary),
            ],
          ),
        );
      }
      final offline = connection is Offline;
      return _MessagePanel(
        icon: Icons.inbox_outlined,
        title: 'No hay agentes en herdr',
        detail: offline
            ? (state == null
                  ? 'Sin conexión: no hay un estado guardado que mostrar.'
                  : 'El último estado guardado no tenía agentes.')
            : 'Cuando un agente se ejecute en herdr aparecerá aquí.',
        actionLabel: 'Nueva conversación',
        onAction: () => _openNewConversation(),
        secondaryLabel: 'Reintentar',
        onSecondary: () async {
          _triage.start();
          await _triage.refreshNow();
        },
      );
    }

    final showBanner = connection is Offline;
    final rows = <Widget>[];
    if (showBanner) {
      rows.add(
        _OfflineBanner(
          reason: connection.reason,
          updatedAt: connection.updatedAt,
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
      children: [_header(), ...rows],
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

class _Shell extends StatelessWidget {
  const _Shell({
    required this.subtitle,
    required this.child,
    this.onRefresh,
    this.onNewConversation,
  });

  final String subtitle;
  final Widget child;
  final VoidCallback? onRefresh;
  final VoidCallback? onNewConversation;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: context.colors.bg,
      body: SafeArea(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(
                Spacing.lg,
                Spacing.lg,
                Spacing.sm,
                Spacing.md,
              ),
              child: Row(
                children: [
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text('Equipo', style: context.text.headline),
                        const SizedBox(height: 2),
                        Text(subtitle, style: context.text.small),
                      ],
                    ),
                  ),
                  IconButton(
                    tooltip: 'Nueva conversación',
                    onPressed: onNewConversation,
                    icon: const Icon(Icons.add_comment_outlined),
                    color: context.colors.textSecondary,
                  ),
                  IconButton(
                    tooltip: 'Actualizar',
                    onPressed: onRefresh,
                    icon: const Icon(Icons.refresh),
                    color: context.colors.textSecondary,
                  ),
                ],
              ),
            ),
            Expanded(child: child),
          ],
        ),
      ),
    );
  }
}

class _OfflineBanner extends StatelessWidget {
  const _OfflineBanner({required this.reason, required this.updatedAt});

  final String? reason;
  final DateTime? updatedAt;

  @override
  Widget build(BuildContext context) {
    return Container(
      margin: const EdgeInsets.fromLTRB(Spacing.lg, 0, Spacing.lg, Spacing.sm),
      padding: const EdgeInsets.all(Spacing.md),
      decoration: BoxDecoration(
        color: context.colors.raised,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: context.colors.outlineSoft),
      ),
      child: Row(
        children: [
          Icon(
            Icons.cloud_off_outlined,
            size: 18,
            color: context.colors.textSecondary,
          ),
          const SizedBox(width: Spacing.sm),
          Expanded(
            child: Text(
              'Sin conexión — '
              '${reason == null || reason!.isEmpty ? 'mostrando el último estado' : reason} '
              '(${ageLabel(updatedAt)}).',
              style: context.text.small,
            ),
          ),
        ],
      ),
    );
  }
}

class _MessagePanel extends StatelessWidget {
  const _MessagePanel({
    required this.icon,
    required this.title,
    required this.detail,
    required this.actionLabel,
    required this.onAction,
    this.hint,
    this.secondaryLabel,
    this.onSecondary,
  });

  final IconData icon;
  final String title;
  final String detail;
  final String? hint;
  final String actionLabel;
  final VoidCallback onAction;
  final String? secondaryLabel;
  final VoidCallback? onSecondary;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(Spacing.xl),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 40, color: context.colors.textMuted),
            const SizedBox(height: Spacing.md),
            Text(title, style: context.text.title, textAlign: TextAlign.center),
            const SizedBox(height: Spacing.sm),
            Text(
              detail,
              style: context.text.bodySecondary,
              textAlign: TextAlign.center,
            ),
            if (hint != null) ...[
              const SizedBox(height: Spacing.sm),
              Text(
                hint!,
                style: context.text.small,
                textAlign: TextAlign.center,
              ),
            ],
            const SizedBox(height: Spacing.lg),
            FilledButton(onPressed: onAction, child: Text(actionLabel)),
            if (secondaryLabel != null && onSecondary != null) ...[
              const SizedBox(height: Spacing.sm),
              OutlinedButton(
                onPressed: onSecondary,
                child: Text(secondaryLabel!),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
