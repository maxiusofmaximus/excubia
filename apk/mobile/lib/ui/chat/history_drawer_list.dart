part of 'history_drawer.dart';

extension _HistoryDrawerList on _HistoryDrawerState {
  String _emptyText() {
    if (_chat.query.isNotEmpty) return 'Nada coincide con «${_chat.query}».';
    return switch (_chat.filter) {
      HistoryFilter.attention => 'Nada pendiente de ti.',
      HistoryFilter.working => 'Ningún agente está trabajando ahora.',
      _ => 'No hay conversaciones en este filtro.',
    };
  }

  Widget _list(BuildContext context) {
    final conversations = _chat.conversations;
    if (conversations.isEmpty && _chat.filter != HistoryFilter.sections) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(Spacing.xl),
          child: Text(
            _emptyText(),
            style: context.text.bodySecondary,
            textAlign: TextAlign.center,
          ),
        ),
      );
    }
    if (_chat.filter == HistoryFilter.sections) {
      return _sectionList(context, conversations);
    }
    final pinned = conversations.where((chat) => chat.pinned).toList();
    final rest = conversations.where((chat) => !chat.pinned).toList();
    final rows = <Widget>[];
    if (_chat.filter == HistoryFilter.all && pinned.isNotEmpty) {
      rows.add(_group(context, 'FIJADOS'));
      rows.addAll(pinned.map(_row));
      rows.add(_group(context, 'RECIENTES'));
      rows.addAll(rest.map(_row));
    } else {
      rows.addAll(conversations.map(_row));
    }
    return ListView(children: rows);
  }

  Widget _sectionList(
    BuildContext context,
    List<ChatConversation> conversations,
  ) {
    final sections = _chat.sections;
    final rows = <Widget>[];
    for (final section in sections) {
      final inSection = conversations
          .where((chat) => chat.sectionId == section.id)
          .toList();
      if (inSection.isEmpty) continue;
      rows.add(_group(context, section.name.toUpperCase()));
      rows.addAll(inSection.map(_row));
    }
    final loose = conversations
        .where((chat) => chat.sectionId == null)
        .toList();
    if (loose.isNotEmpty) {
      rows.add(_group(context, 'SIN SECCIÓN'));
      rows.addAll(loose.map(_row));
    }
    if (rows.isEmpty) {
      rows.add(
        Padding(
          padding: const EdgeInsets.all(Spacing.xl),
          child: Text(
            'Todavía no hay secciones con conversaciones.',
            style: context.text.bodySecondary,
            textAlign: TextAlign.center,
          ),
        ),
      );
    }
    return ListView(children: rows);
  }

  Widget _group(BuildContext context, String label) => Padding(
    padding: const EdgeInsets.fromLTRB(
      Spacing.md,
      Spacing.md,
      Spacing.md,
      Spacing.xs,
    ),
    child: Text(
      label,
      style: context.text.label.copyWith(
        color: context.colors.textMuted,
        letterSpacing: 0.8,
      ),
    ),
  );

  Widget _row(ChatConversation conversation) {
    final context = this.context;
    final c = context.colors;
    final selected = _chat.current?.id == conversation.id;
    return Semantics(
      container: true,
      label: '${conversation.title}. Abrir conversación.',
      child: Material(
        color: selected ? c.accentSoft : Colors.transparent,
        child: InkWell(
          onTap: () {
            Navigator.of(context).maybePop();
            _chat.openConversation(conversation.id);
          },
          child: Container(
            constraints: const BoxConstraints(minHeight: 56),
            padding: const EdgeInsets.fromLTRB(Spacing.md, 4, 4, 4),
            child: Row(
              children: [
                if (conversation.pinned)
                  Semantics(
                    label: 'Fijado',
                    child: Icon(Icons.star, size: 16, color: c.accentText),
                  ),
                if (conversation.pinned) const SizedBox(width: Spacing.xs),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      Text(
                        conversation.title,
                        style: context.text.body,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      Text(
                        conversation.activeRuns > 0
                            ? '${_chat.alfredName} · '
                                  '${_chat.progressOf(conversation.id) ?? 'trabajando…'}'
                            : '${_chat.alfredName} · '
                                  '${relativeTime(conversation.lastMessageAt)}',
                        style: context.text.small,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ],
                  ),
                ),
                if (conversation.unread > BigInt.zero)
                  Badge(
                    label: Text('${conversation.unread}'),
                    offset: const Offset(-6, 0),
                  ),
                PopupMenuButton<String>(
                  tooltip: 'Acciones de la conversación',
                  icon: Icon(Icons.more_vert, color: c.textMuted),
                  onSelected: (action) => _action(action, conversation),
                  itemBuilder: (context) => [
                    const PopupMenuItem(
                      value: 'rename',
                      child: Text('Renombrar'),
                    ),
                    PopupMenuItem(
                      value: 'pin',
                      child: Text(
                        conversation.pinned ? 'Quitar de fijados' : 'Fijar',
                      ),
                    ),
                    const PopupMenuItem(
                      value: 'move',
                      child: Text('Mover a sección'),
                    ),
                    PopupMenuItem(
                      value: conversation.archived ? 'unarchive' : 'archive',
                      child: Text(
                        conversation.archived ? 'Desarchivar' : 'Archivar',
                      ),
                    ),
                    PopupMenuItem(
                      value: 'delete',
                      child: Row(
                        children: [
                          Icon(Icons.delete_outline, color: c.statusBlocked),
                          const SizedBox(width: Spacing.sm),
                          Text(
                            'Borrar',
                            style: TextStyle(color: c.statusBlocked),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
