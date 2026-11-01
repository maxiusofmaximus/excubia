import 'package:flutter/material.dart';

import '../../core/api/agenda_api.dart';
import '../../core/state/agenda_list_controller.dart';
import '../theme.dart';
import 'agenda_format.dart';
import 'agenda_list_view.dart';
import 'agenda_note_screen.dart';

/// «Notas»: the notes the app and Alfred keep, the newest first.
class AgendaNotesScreen extends StatefulWidget {
  const AgendaNotesScreen({super.key, required this.api});

  final AgendaApi api;

  @override
  State<AgendaNotesScreen> createState() => _AgendaNotesScreenState();
}

class _AgendaNotesScreenState extends State<AgendaNotesScreen> {
  late final AgendaListController<AgendaNoteRow> _list = AgendaListController(
    (query, before) => widget.api.notes(query: query, before: before),
  );

  @override
  void dispose() {
    _list.dispose();
    super.dispose();
  }

  /// A new note, or the editor of [row]. The list is read again when it closes,
  /// since the note may have changed or gone.
  Future<void> _open([AgendaNoteRow? row]) async {
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => AgendaNoteScreen(api: widget.api, id: row?.id),
      ),
    );
    if (mounted) await _list.reload();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(title: const Text('Notas')),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _open,
        icon: const Icon(Icons.add),
        label: const Text('Nueva nota'),
      ),
      body: AgendaListView<AgendaNoteRow>(
        list: _list,
        hint: 'Buscar en asuntos y textos…',
        empty: 'Aún no hay notas. Crea una con «Nueva nota».',
        rowBuilder: (context, row) => ListTile(
          minVerticalPadding: Spacing.md,
          title: Text(
            noteTitle(row),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
          subtitle: Text(
            row.excerpt,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
          ),
          trailing: Text(
            relativeDate(row.updatedAt, DateTime.now()),
            style: context.text.small,
          ),
          onTap: () => _open(row),
        ),
      ),
    );
  }
}
