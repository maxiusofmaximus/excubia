import 'dart:async';

import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/error.dart';
import 'package:herdr_notify/src/rust/api/state.dart';
import 'package:herdr_notify/src/rust/api/workspace.dart';
import 'package:herdr_notify/src/rust/api/write.dart';

import '../core/model/connection_state.dart';
import '../core/state/live_triage.dart';
import 'new_conversation_screen.dart';
import 'theme.dart';
import 'widgets/agent_composer.dart';
import 'widgets/surface_card.dart';
import 'widgets/status_dot.dart';

/// What the detail reports back to the list when it leaves.
sealed class AgentDetailResult {
  const AgentDetailResult();
}

class AgentStopped extends AgentDetailResult {
  const AgentStopped({required this.alreadyGone});

  final bool alreadyGone;
}

class ConversationCreated extends AgentDetailResult {
  const ConversationCreated(this.created);

  final CreatedWorkspace created;
}

/// Read-only data plus respond, keys, mark seen and stop. Controls are enabled
/// only while the agent is blocked and its process is alive.
class AgentDetailScreen extends StatefulWidget {
  const AgentDetailScreen({
    super.key,
    required this.triage,
    required this.paneId,
  });

  final LiveTriage triage;
  final String paneId;

  @override
  State<AgentDetailScreen> createState() => _AgentDetailScreenState();
}

class _AgentDetailScreenState extends State<AgentDetailScreen> {
  final TextEditingController _message = TextEditingController();

  bool _sending = false;
  bool _stopping = false;
  String? _result;
  bool _resultOk = true;
  ErrorInfo? _error;
  String? _seenKey;

  bool get _busy => _sending || _stopping;

  @override
  void initState() {
    super.initState();
    widget.triage.addListener(_onStateChanged);
    _maybeMarkSeen();
  }

  @override
  void dispose() {
    widget.triage.removeListener(_onStateChanged);
    _message.dispose();
    super.dispose();
  }

  void _onStateChanged() {
    if (!mounted) return;
    setState(() {});
  }

  AgentEntry? _agent() {
    final state = widget.triage.currentState;
    if (state == null) return null;
    for (final agent in state.agents) {
      if (agent.paneId == widget.paneId) return agent;
    }
    return null;
  }

  /// Opening a `done` row marks it seen once. It runs when the detail opens,
  /// not on every live update: an update caused by the user's own response must
  /// not replace its confirmation.
  void _maybeMarkSeen() {
    final agent = _agent();
    if (agent == null || agent.restored || agent.status != AgentStatus.done) {
      return;
    }
    final revision = widget.triage.currentState?.revision ?? 0;
    final key = '${agent.paneId}:$revision';
    if (_seenKey == key) return;
    _seenKey = key;
    unawaited(
      _send(
        AgentAction(
          kind: AgentActionKind.seen,
          paneId: agent.paneId,
          keys: const <String>[],
          confirm: false,
        ),
        'Marcado como visto.',
      ),
    );
  }

  Future<bool> _send(
    AgentAction action,
    String success, {
    bool clearDraft = false,
  }) async {
    if (_busy) return false;
    setState(() {
      _sending = true;
      _error = null;
      _result = 'Enviando…';
      _resultOk = true;
    });
    final error = await widget.triage.write(action);
    if (!mounted) return false;
    setState(() {
      _sending = false;
      _error = error;
      _result = error == null ? success : null;
      _resultOk = error == null;
      if (error == null && clearDraft) _message.clear();
    });
    return error == null;
  }

  Future<void> _respond() async {
    final text = _message.text.trim();
    if (text.isEmpty) {
      setState(() {
        _error = null;
        _result = 'Escribe un mensaje antes de enviar.';
        _resultOk = false;
      });
      return;
    }
    await _send(
      AgentAction(
        kind: AgentActionKind.respond,
        paneId: widget.paneId,
        text: text,
        keys: const <String>[],
        confirm: false,
      ),
      'Respuesta enviada. El agente la recibirá en su terminal.',
      clearDraft: true,
    );
  }

  Future<void> _sendKey(String key) => _send(
    AgentAction(
      kind: AgentActionKind.keys,
      paneId: widget.paneId,
      keys: <String>[key],
      confirm: false,
    ),
    'Tecla enviada. El agente la recibirá en su terminal.',
  );

  Future<void> _confirmStop(AgentEntry agent) async {
    if (_busy) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: context.colors.raised,
        title: const Text('Detener agente'),
        content: Text(
          'Se cerrará el proceso de ${agent.title ?? agent.project} '
          '(${agent.paneId}). El panel saldrá de la lista y la acción no se '
          'puede deshacer.',
          style: context.text.bodySecondary,
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancelar'),
          ),
          FilledButton(
            style: FilledButton.styleFrom(
              minimumSize: const Size(88, 44),
              backgroundColor: context.colors.statusBlocked,
              foregroundColor: Colors.white,
            ),
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Detener'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    setState(() {
      _stopping = true;
      _error = null;
      _result = null;
    });
    final result = await widget.triage.stopAgent(widget.paneId);
    if (!mounted) return;
    if (result.stopped) {
      Navigator.of(context).pop(const AgentStopped(alreadyGone: false));
      return;
    }
    if (result.gone) {
      Navigator.of(context).pop(const AgentStopped(alreadyGone: true));
      return;
    }
    setState(() {
      _stopping = false;
      _error = result.error;
    });
  }

  Future<void> _newHere(AgentEntry agent) async {
    final cwd = agent.cwd;
    if (cwd == null || cwd.isEmpty) return;
    final created = await Navigator.of(context).push<CreatedWorkspace>(
      MaterialPageRoute<CreatedWorkspace>(
        builder: (_) =>
            NewConversationScreen(triage: widget.triage, initialCwd: cwd),
      ),
    );
    if (created != null && mounted) {
      Navigator.of(context).pop(ConversationCreated(created));
    }
  }

  @override
  Widget build(BuildContext context) {
    final agent = _agent();
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(
        leading: IconButton(
          icon: const Icon(Icons.arrow_back),
          onPressed: () => Navigator.of(context).pop(),
        ),
        title: const Text('Detalle del agente'),
      ),
      body: agent == null ? _missing() : _detail(agent),
    );
  }

  Widget _missing() {
    final offline = widget.triage.connection is Offline;
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(Spacing.xl),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.search_off, size: 40, color: context.colors.textMuted),
            const SizedBox(height: Spacing.md),
            Text(
              _error?.message ??
                  (offline
                      ? 'Sin conexión: este agente no está en el último '
                            'estado guardado.'
                      : 'Este agente ya no está en herdr.'),
              style: context.text.bodySecondary,
              textAlign: TextAlign.center,
            ),
            if (_error?.action != null) ...[
              const SizedBox(height: Spacing.sm),
              Text(
                _error!.action!,
                style: context.text.small,
                textAlign: TextAlign.center,
              ),
            ],
          ],
        ),
      ),
    );
  }

  Widget _detail(AgentEntry agent) {
    final ready =
        agent.status == AgentStatus.blocked && !agent.restored && !_busy;
    final hint = agent.restored
        ? 'El agente terminó; este panel es una instantánea de su última '
              'pantalla.'
        : ready
        ? 'El agente está esperando; tu respuesta llega al terminal.'
        : 'Disponible cuando el agente está bloqueado '
              '(ahora: ${context.colors.statusLabel(agent.status).toLowerCase()}).';
    return ListView(
      padding: const EdgeInsets.fromLTRB(
        Spacing.lg,
        Spacing.lg,
        Spacing.lg,
        Spacing.xl,
      ),
      children: [
        Row(
          children: [
            StatusDot(status: agent.status),
            const SizedBox(width: Spacing.md),
            Expanded(
              child: Text(
                agent.project,
                style: context.text.title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
            ),
          ],
        ),
        const SizedBox(height: Spacing.xs),
        Text(
          context.colors.statusLabel(agent.status),
          style: context.text.label.copyWith(
            color: context.colors.labelForStatus(agent.status),
          ),
        ),
        if (agent.restored) ...[
          const SizedBox(height: Spacing.sm),
          Text(
            'Restaurado de una sesión anterior; su proceso ya no corre.',
            style: context.text.small,
          ),
        ],
        const SizedBox(height: Spacing.lg),
        AgentComposer(
          controller: _message,
          ready: ready,
          sending: _sending,
          stopping: _stopping,
          canStop: !agent.restored,
          hint: hint,
          onRespond: _respond,
          onKey: _sendKey,
          onStop: () => _confirmStop(agent),
          result: _result,
          resultOk: _resultOk,
          error: _error,
        ),
        if (agent.restored) ...[
          const SizedBox(height: Spacing.lg),
          SurfaceCard(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('Panel restaurado', style: context.text.title),
                const SizedBox(height: Spacing.xs),
                Text(
                  'El agente terminó. Puedes abrir una conversación nueva en la '
                  'misma carpeta del equipo.',
                  style: context.text.bodySecondary,
                ),
                const SizedBox(height: Spacing.md),
                OutlinedButton.icon(
                  onPressed: agent.cwd == null ? null : () => _newHere(agent),
                  icon: const Icon(Icons.add_comment_outlined),
                  label: const Text('Nueva conversación aquí'),
                ),
                if (agent.cwd == null) ...[
                  const SizedBox(height: Spacing.sm),
                  Text(
                    'Sin carpeta conocida para reutilizar.',
                    style: context.text.small,
                  ),
                ],
              ],
            ),
          ),
        ],
        const SizedBox(height: Spacing.lg),
        _fields(agent),
        const SizedBox(height: Spacing.lg),
        Text(
          'Solo se escriben respuestas y teclas permitidas; no hay chat ni '
          'adjuntos en esta fase.',
          style: context.text.small,
        ),
      ],
    );
  }

  Widget _fields(AgentEntry agent) {
    final revision = widget.triage.currentState?.revision ?? 0;
    return SurfaceCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _Field(label: 'Tarea', value: agent.title ?? '—'),
          _Field(label: 'Agente', value: agent.agent),
          _Field(label: 'Workspace', value: agent.workspaceId),
          _Field(label: 'Pane', value: agent.paneId, mono: true),
          if (agent.cwd != null)
            _Field(label: 'Carpeta', value: agent.cwd!, mono: true),
          _Field(label: 'Atención', value: '${agent.attention} de 3'),
          _Field(label: 'En foco', value: agent.focused ? 'Sí' : 'No'),
          _Field(label: 'Puede reanudar', value: agent.canResume ? 'Sí' : 'No'),
          _Field(label: 'Revisión del estado', value: '$revision'),
        ],
      ),
    );
  }
}

class _Field extends StatelessWidget {
  const _Field({required this.label, required this.value, this.mono = false});

  final String label;
  final String value;
  final bool mono;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: Spacing.md),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(label, style: context.text.label),
          const SizedBox(height: 2),
          Text(value, style: mono ? context.text.mono : context.text.body),
        ],
      ),
    );
  }
}
