part of 'chat_screen.dart';

class _EmptyChat extends StatelessWidget {
  const _EmptyChat({
    required this.chat,
    required this.hires,
    required this.onSuggestion,
    required this.onOpenTeam,
  });

  final ChatController chat;
  final HiresController hires;
  final ValueChanged<String> onSuggestion;
  final VoidCallback onOpenTeam;

  @override
  Widget build(BuildContext context) {
    final recents = chat.conversations.take(3).toList();
    return ListView(
      padding: const EdgeInsets.fromLTRB(
        Spacing.lg,
        Spacing.xl,
        Spacing.lg,
        Spacing.xl,
      ),
      children: [
        Center(
          child: AgentAvatar(name: chat.alfredName, size: 68, glance: true),
        ),
        const SizedBox(height: Spacing.md),
        Text(
          '¿Qué movemos hoy?',
          style: context.text.headline,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Spacing.sm),
        Text(
          'Dime un objetivo y reparto el trabajo entre Ernesto y su equipo. '
          'Yo te traigo el resultado.',
          style: context.text.bodySecondary,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Spacing.lg),
        _Suggestions(chat: chat, hires: hires, onSuggestion: onSuggestion),
        const SizedBox(height: Spacing.lg),
        _TeamStrip(chat: chat, hires: hires, onTap: onOpenTeam),
        if (recents.isNotEmpty) ...[
          const SizedBox(height: Spacing.lg),
          Text('RECIENTES', style: context.text.label),
          const SizedBox(height: Spacing.xs),
          for (final conversation in recents)
            _RecentRow(
              conversation: conversation,
              onTap: () => chat.openConversation(conversation.id),
            ),
        ],
      ],
    );
  }
}

class _Suggestions extends StatelessWidget {
  const _Suggestions({
    required this.chat,
    required this.hires,
    required this.onSuggestion,
  });

  final ChatController chat;
  final HiresController hires;
  final ValueChanged<String> onSuggestion;

  static const _icons = [
    Icons.help_outline,
    Icons.work_outline,
    Icons.edit_note,
  ];

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: hires,
      builder: (context, _) {
        // Cambia al crear chats, así no salen siempre las mismas frases.
        final suggestions = chatSuggestions(
          hires: hires.hires,
          rotation: chat.conversations.length,
        );
        return Column(
          children: [
            for (final (index, suggestion) in suggestions.indexed)
              _Suggestion(
                icon: _icons[index],
                title: suggestion.text,
                detail: suggestion.detail,
                onTap: () => onSuggestion(suggestion.text),
              ),
          ],
        );
      },
    );
  }
}

class _Suggestion extends StatelessWidget {
  const _Suggestion({
    required this.icon,
    required this.title,
    required this.detail,
    required this.onTap,
  });

  final IconData icon;
  final String title;
  final String detail;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Semantics(
      button: true,
      label: '$title, $detail',
      child: Material(
        color: c.surface,
        borderRadius: BorderRadius.circular(16),
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(16),
          child: Container(
            constraints: const BoxConstraints(minHeight: 56),
            margin: const EdgeInsets.only(bottom: Spacing.sm),
            padding: const EdgeInsets.all(Spacing.md),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(16),
              border: Border.all(color: c.outlineSoft),
            ),
            child: Row(
              children: [
                Container(
                  width: 30,
                  height: 30,
                  decoration: BoxDecoration(
                    color: c.accentSoft,
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Icon(icon, size: 18, color: c.accentText),
                ),
                const SizedBox(width: Spacing.sm),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(title, style: context.text.body),
                      Text(detail, style: context.text.small),
                    ],
                  ),
                ),
                Icon(Icons.arrow_forward, size: 18, color: c.textMuted),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _TeamStrip extends StatelessWidget {
  const _TeamStrip({
    required this.chat,
    required this.hires,
    required this.onTap,
  });

  final ChatController chat;
  final HiresController hires;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: hires,
      builder: (context, _) => _strip(context),
    );
  }

  Widget _strip(BuildContext context) {
    final c = context.colors;
    final agents = chat.teamAgents;
    final working = agents
        .where((agent) => agent.status.name == 'working')
        .length;
    final blocked = agents
        .where((agent) => agent.status.name == 'blocked')
        .length;
    final hired = hires.hires.length;
    final detail = hired > 0
        ? teamCountLabel(hired: hired, active: agents.length)
        : agents.isEmpty
        ? 'Sin agentes'
        : '$working trabajando · $blocked esperan';
    return Semantics(
      button: true,
      label: 'Equipo: $detail. Abrir Equipo.',
      child: Material(
        color: c.raised,
        borderRadius: BorderRadius.circular(16),
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(16),
          child: Container(
            constraints: const BoxConstraints(minHeight: 56),
            padding: const EdgeInsets.all(Spacing.md),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(16),
              border: Border.all(color: c.outlineSoft),
            ),
            child: Row(
              children: [
                Icon(Icons.groups_outlined, size: 22, color: c.accentText),
                const SizedBox(width: Spacing.sm),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('Equipo', style: context.text.title),
                      Text(detail, style: context.text.small),
                    ],
                  ),
                ),
                Icon(Icons.chevron_right, size: 20, color: c.textMuted),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _RecentRow extends StatelessWidget {
  const _RecentRow({required this.conversation, required this.onTap});

  final ChatConversation conversation;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Semantics(
      button: true,
      label: 'Abrir ${conversation.title}',
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: onTap,
          child: Container(
            constraints: const BoxConstraints(minHeight: 48),
            padding: const EdgeInsets.symmetric(vertical: Spacing.xs),
            child: Row(
              children: [
                Icon(Icons.chat_bubble_outline, size: 18, color: c.textMuted),
                const SizedBox(width: Spacing.sm),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        conversation.title,
                        style: context.text.body,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      Text(
                        relativeTime(conversation.lastMessageAt),
                        style: context.text.small,
                      ),
                    ],
                  ),
                ),
                if (conversation.unread > BigInt.zero)
                  Badge(label: Text('${conversation.unread}')),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
