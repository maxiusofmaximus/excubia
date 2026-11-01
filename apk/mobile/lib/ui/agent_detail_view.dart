part of 'agent_detail_screen.dart';

extension _AgentDetailView on _AgentDetailScreenState {
  Widget _missing() {
    final offline = widget.triage.connection is Offline;
    final failure = widget.triage.connectionFailure;
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
                  (!offline
                      ? 'Este agente ya no está disponible.'
                      : failure == null
                      ? 'Sin conexión: este agente no está en el último '
                            'estado guardado.'
                      : connectionText(failure)),
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
        StatusChip(status: agent.status),
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
