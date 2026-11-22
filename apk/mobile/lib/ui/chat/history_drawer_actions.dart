part of 'history_drawer.dart';

extension _HistoryDrawerActions on _HistoryDrawerState {
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
      case 'delete':
        if (await _confirmDelete()) await _delete(conversation);
    }
  }

  Future<bool> _confirmDelete() async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('¿Borrar esta conversación?'),
        content: const Text(
          'Se borra para siempre, con sus mensajes y adjuntos. Las notas que '
          'Alfred guardó se conservan. No se puede deshacer.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancelar'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            style: FilledButton.styleFrom(
              backgroundColor: context.colors.statusBlocked,
              foregroundColor: context.colors.onAttention,
            ),
            child: const Text('Borrar'),
          ),
        ],
      ),
    );
    return confirmed ?? false;
  }

  Future<void> _delete(ChatConversation conversation) async {
    final failure = await _chat.deleteConversation(conversation);
    if (failure != null) {
      _report(failure);
    } else if (mounted) {
      ScaffoldMessenger.of(context)
          .showSnackBar(const SnackBar(content: Text('Conversación borrada')));
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
