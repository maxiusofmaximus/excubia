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
    // A finished agent (restored) gets no respond panel at all: only the new conversation below.
    return ListView(
      padding: const EdgeInsets.fromLTRB(
        Spacing.lg,
        Spacing.lg,
        Spacing.lg,
        Spacing.xl,
      ),
      children: [
        Center(
          child: SizedBox(
            width: 240,
            child: AgentPortraitCard(
              id: agentProfileFor(agentNameOf(agent))?.id ?? '',
              name: stripRoleSuffix(agentNameOf(agent)),
              // An unknown state adds nothing: the line is the project alone.
              tagline: [
                if (agentHasProject(agent)) agent.project,
                if (agent.status != AgentStatus.unknown)
                  context.colors.statusLabel(agent.status),
                ?agentEndedText(agent),
              ].where((part) => part.trim().isNotEmpty).join(' · '),
            ),
          ),
        ),
        const SizedBox(height: Spacing.lg),
        if (!agent.restored) ...[
          const SizedBox(height: Spacing.lg),
          AgentComposer(
            controller: _message,
            ready: ready,
            sending: _sending,
            stopping: _stopping,
            canStop: true,
            blocked: agent.status == AgentStatus.blocked,
            onRespond: _respond,
            onKey: _sendKey,
            onStop: () => _confirmStop(agent),
            result: _result,
            resultOk: _resultOk,
            error: _error,
          ),
        ],
        if (agent.restored) ...[
          const SizedBox(height: Spacing.lg),
          SurfaceCard(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('Esta conversación terminó', style: context.text.title),
                const SizedBox(height: Spacing.xs),
                Text(
                  'Puedes abrir una conversación nueva en la misma carpeta del '
                  'equipo.',
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
        SurfaceCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('Volver a una versión anterior', style: context.text.title),
              const SizedBox(height: Spacing.xs),
              Text(
                'Mira qué cambió el agente y, si hace falta, recupera una '
                'versión anterior de la carpeta.',
                style: context.text.bodySecondary,
              ),
              const SizedBox(height: Spacing.md),
              OutlinedButton.icon(
                style: OutlinedButton.styleFrom(minimumSize: const Size(0, 48)),
                onPressed: () => _openRestore(agent),
                icon: const Icon(Icons.history),
                label: const Text('Ver versiones'),
              ),
            ],
          ),
        ),
        const SizedBox(height: Spacing.lg),
        _details(agent),
      ],
    );
  }

  /// The technical fields and the scope note: for people who need them, so folded and shut.
  Widget _details(AgentEntry agent) {
    return Theme(
      data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
      child: ExpansionTile(
        key: const ValueKey('agent-details'),
        title: Text('Detalles', style: context.text.body),
        tilePadding: EdgeInsets.zero,
        childrenPadding: const EdgeInsets.only(top: Spacing.sm),
        shape: const Border(),
        collapsedShape: const Border(),
        iconColor: context.colors.textSecondary,
        collapsedIconColor: context.colors.textSecondary,
        expandedCrossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _fields(agent),
          const SizedBox(height: Spacing.lg),
          Text(
            'Solo se escriben respuestas y teclas permitidas; no hay chat ni '
            'adjuntos en esta fase.',
            style: context.text.small,
          ),
        ],
      ),
    );
  }

  Widget _fields(AgentEntry agent) {
    final revision = widget.triage.currentState?.revision ?? 0;
    return SurfaceCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _Field(label: 'Tarea', value: stripRoleSuffix(agent.title ?? '—')),
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
