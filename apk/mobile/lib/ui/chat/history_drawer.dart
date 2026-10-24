import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';
import 'package:herdr_notify/src/rust/api/error/chat.dart';

import '../../core/state/chat_controller.dart';
import '../theme.dart';
import '../widgets/time_label.dart';

/// `wf41`: the drawer keeps the chat on screen while the history, its filters
/// and the per-row actions are one tap away.
class HistoryDrawer extends StatefulWidget {
  const HistoryDrawer({
    super.key,
    required this.controller,
    required this.onOpenTeam,
  });

  final ChatController controller;
  final VoidCallback onOpenTeam;

  @override
  State<HistoryDrawer> createState() => _HistoryDrawerState();
}

class _HistoryDrawerState extends State<HistoryDrawer> {
  late final TextEditingController _search = TextEditingController(
    text: widget.controller.query,
  );

  ChatController get _chat => widget.controller;

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Drawer(
      width: 330,
      backgroundColor: c.surface,
      child: SafeArea(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _header(context),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: Spacing.md),
              child: TextField(
                controller: _search,
                onChanged: _chat.setQuery,
                textInputAction: TextInputAction.search,
                style: context.text.body,
                decoration: const InputDecoration(
                  hintText: 'Buscar en títulos y mensajes…',
                  prefixIcon: Icon(Icons.search),
                  isDense: true,
                ),
              ),
            ),
            Padding(
              padding: const EdgeInsets.all(Spacing.md),
              child: FilledButton.icon(
                onPressed: _newChat,
                icon: const Icon(Icons.add),
                label: const Text('Nuevo chat con Alfred'),
              ),
            ),
            _filters(context),
            const Divider(height: 1),
            Expanded(child: _list(context)),
            const Divider(height: 1),
            Wrap(
              alignment: WrapAlignment.spaceBetween,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                TextButton(
                  onPressed: () {
                    _chat.setFilter(HistoryFilter.archived);
                    Navigator.of(context).maybePop();
                  },
                  child: Text('Ver archivados (${_chat.archivedCount})'),
                ),
                TextButton(
                  onPressed: () {
                    Navigator.of(context).maybePop();
                    widget.onOpenTeam();
                  },
                  child: const Text('Equipo →'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _header(BuildContext context) {
    final c = context.colors;
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        Spacing.lg,
        Spacing.md,
        Spacing.sm,
        Spacing.sm,
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('Historial', style: context.text.headline),
                Text(
                  '${_chat.pinnedCount} fijados · '
                  '${_chat.conversations.length} '
                  '${_chat.conversations.length == 1 ? 'visible' : 'visibles'} · '
                  '${_chat.archivedCount} archivados',
                  style: context.text.small,
                ),
              ],
            ),
          ),
          IconButton(
            tooltip: 'Cerrar historial',
            onPressed: () => Navigator.of(context).maybePop(),
            icon: const Icon(Icons.close),
            color: c.textSecondary,
          ),
        ],
      ),
    );
  }

  Widget _filters(BuildContext context) {
    const labels = <HistoryFilter, String>{
      HistoryFilter.all: 'Todos',
      HistoryFilter.pinned: 'Fijados',
      HistoryFilter.sections: 'Secciones',
      HistoryFilter.archived: 'Archivados',
    };
    return SizedBox(
      height: 56,
      child: ListView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: Spacing.md),
        children: [
          for (final entry in labels.entries)
            Padding(
              padding: const EdgeInsets.only(right: Spacing.sm),
              child: ChoiceChip(
                label: Text(entry.value),
                selected: _chat.filter == entry.key,
                onSelected: (_) => _chat.setFilter(entry.key),
                labelStyle: context.text.small,
                selectedColor: context.colors.accentSoft,
                backgroundColor: context.colors.raised,
                side: BorderSide(color: context.colors.outlineSoft),
                padding: const EdgeInsets.symmetric(
                  horizontal: Spacing.sm,
                  vertical: Spacing.md,
                ),
              ),
            ),
        ],
      ),
    );
  }

  Widget _list(BuildContext context) {
    final conversations = _chat.conversations;
    if (conversations.isEmpty && _chat.filter != HistoryFilter.sections) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(Spacing.xl),
          child: Text(
            _chat.query.isEmpty
                ? 'No hay conversaciones en este filtro.'
                : 'Nada coincide con «${_chat.query}».',
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
                    child: Icon(Icons.star, size: 16, color: c.accent),
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
                        '${_chat.alfredName} · '
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
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Future<void> _action(String action, ChatConversation conversation) async {
    switch (action) {
      case 'rename':
        final title = await _askText(
          title: 'Renombrar conversación',
          label: 'Título',
          initial: conversation.title,
        );
        if (title == null) return;
        _report(await _chat.rename(conversation.id, title));
      case 'pin':
        _report(await _chat.togglePin(conversation));
      case 'move':
        await _move(conversation);
      case 'archive':
        _report(await _chat.archive(conversation));
      case 'unarchive':
        _report(await _chat.archive(conversation));
    }
  }

  Future<void> _move(ChatConversation conversation) async {
    final sectionId = await showModalBottomSheet<String?>(
      context: context,
      backgroundColor: context.colors.surface,
      builder: (sheet) => SafeArea(
        child: ListView(
          shrinkWrap: true,
          children: [
            Padding(
              padding: const EdgeInsets.all(Spacing.lg),
              child: Text('Mover a sección', style: context.text.title),
            ),
            ListTile(
              minVerticalPadding: Spacing.md,
              leading: const Icon(Icons.inbox_outlined),
              title: const Text('Sin sección'),
              onTap: () => Navigator.of(sheet).pop(''),
            ),
            for (final section in _chat.sections)
              ListTile(
                minVerticalPadding: Spacing.md,
                leading: const Icon(Icons.folder_outlined),
                title: Text(section.name),
                onTap: () => Navigator.of(sheet).pop(section.id),
              ),
            ListTile(
              minVerticalPadding: Spacing.md,
              leading: const Icon(Icons.create_new_folder_outlined),
              title: const Text('Nueva sección…'),
              onTap: () => Navigator.of(sheet).pop('__new__'),
            ),
          ],
        ),
      ),
    );
    if (sectionId == null) return;
    var target = sectionId;
    if (target == '__new__') {
      final name = await _askText(
        title: 'Nueva sección',
        label: 'Nombre',
        initial: '',
      );
      if (name == null) return;
      final section = await _chat.createSection(name);
      if (section == null) return;
      target = section.id;
    }
    _report(
      await _chat.moveToSection(
        conversation.id,
        target.isEmpty ? null : target,
      ),
    );
  }

  Future<String?> _askText({
    required String title,
    required String label,
    required String initial,
  }) => showDialog<String>(
    context: context,
    builder: (_) =>
        _TextPromptDialog(title: title, label: label, initial: initial),
  );

  Future<void> _newChat() async {
    final id = await _chat.createConversation();
    if (!mounted) return;
    if (id != null) Navigator.of(context).maybePop();
  }

  void _report(ChatFailure? failure) {
    if (failure == null || !mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(
          failure.action == null
              ? failure.message
              : '${failure.message} ${failure.action}',
        ),
      ),
    );
  }
}

class _TextPromptDialog extends StatefulWidget {
  const _TextPromptDialog({
    required this.title,
    required this.label,
    required this.initial,
  });

  final String title;
  final String label;
  final String initial;

  @override
  State<_TextPromptDialog> createState() => _TextPromptDialogState();
}

class _TextPromptDialogState extends State<_TextPromptDialog> {
  late final TextEditingController _controller = TextEditingController(
    text: widget.initial,
  );

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _submit() {
    final text = _controller.text.trim();
    Navigator.of(context).pop(text.isEmpty ? null : text);
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text(widget.title),
      content: TextField(
        controller: _controller,
        autofocus: true,
        maxLength: 200,
        decoration: InputDecoration(labelText: widget.label),
        onSubmitted: (_) => _submit(),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancelar'),
        ),
        FilledButton(onPressed: _submit, child: const Text('Guardar')),
      ],
    );
  }
}
