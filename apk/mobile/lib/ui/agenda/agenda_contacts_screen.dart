import 'package:flutter/material.dart';

import '../../core/api/agenda_api.dart';
import '../../core/api/phone_contacts.dart';
import '../../core/state/agenda_list_controller.dart';
import '../theme.dart';
import 'agenda_contact_screen.dart';
import 'agenda_import_screen.dart';
import 'agenda_list_view.dart';
import 'agenda_widgets.dart';

/// «Contactos»: the people the agenda knows, by name and e-mail. The phone's
/// contacts come in from the import button above the list.
class AgendaContactsScreen extends StatefulWidget {
  const AgendaContactsScreen({
    super.key,
    required this.api,
    required this.phone,
  });

  final AgendaApi api;
  final PhoneContacts phone;

  @override
  State<AgendaContactsScreen> createState() => _AgendaContactsScreenState();
}

class _AgendaContactsScreenState extends State<AgendaContactsScreen> {
  late final AgendaListController<AgendaContact> _list = AgendaListController(
    (query, after) => widget.api.contacts(query: query, after: after),
  );

  @override
  void dispose() {
    _list.dispose();
    super.dispose();
  }

  /// A new contact, or the editor of [contact]; the list is read again on return.
  Future<void> _open([AgendaContact? contact]) async {
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => AgendaContactScreen(api: widget.api, contact: contact),
      ),
    );
    if (mounted) await _list.reload();
  }

  Future<void> _import() async {
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) =>
            AgendaImportScreen(api: widget.api, phone: widget.phone),
      ),
    );
    if (mounted) await _list.reload();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(title: const Text('Contactos')),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _open,
        icon: const Icon(Icons.person_add_alt),
        label: const Text('Nuevo contacto'),
      ),
      body: AgendaListView<AgendaContact>(
        list: _list,
        hint: 'Buscar por nombre o alias…',
        empty: 'Aún no hay contactos. Crea uno o importa los del teléfono.',
        header: Padding(
          padding: const EdgeInsets.symmetric(horizontal: Spacing.md),
          child: OutlinedButton.icon(
            onPressed: _import,
            icon: const Icon(Icons.contacts_outlined),
            label: const Text('Importar contactos del teléfono'),
          ),
        ),
        rowBuilder: (context, contact) {
          final first = contact.addresses.isEmpty
              ? null
              : contact.addresses.first;
          return ListTile(
            minVerticalPadding: Spacing.md,
            title: Text(
              contact.name,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
            subtitle: Text(
              first?.address ?? 'Sin dirección',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
            trailing: SourceTag(source: first?.source),
            onTap: () => _open(contact),
          );
        },
      ),
    );
  }
}
