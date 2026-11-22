import 'package:flutter/material.dart';

import '../../core/api/agenda_api.dart';
import '../theme.dart';
import 'agenda_format.dart';
import 'agenda_widgets.dart';

/// One contact: a new one when [contact] is null, otherwise its editor. The
/// addresses are sent only when they were changed, so the host keeps the origin
/// of the ones that stay. Deleting needs `APPROVE`.
class AgendaContactScreen extends StatefulWidget {
  const AgendaContactScreen({super.key, required this.api, this.contact});

  final AgendaApi api;
  final AgendaContact? contact;

  @override
  State<AgendaContactScreen> createState() => _AgendaContactScreenState();
}

class _AgendaContactScreenState extends State<AgendaContactScreen> {
  late final TextEditingController _name = TextEditingController(
    text: widget.contact?.name,
  );
  late final TextEditingController _aliases = TextEditingController(
    text: widget.contact?.aliases.join('\n'),
  );
  late final TextEditingController _addresses = TextEditingController(
    text: widget.contact?.addresses.map((item) => item.address).join('\n'),
  );
  late final TextEditingController _notes = TextEditingController(
    text: widget.contact?.notes,
  );
  bool _busy = false;
  String? _failure;

  @override
  void dispose() {
    _name.dispose();
    _aliases.dispose();
    _addresses.dispose();
    _notes.dispose();
    super.dispose();
  }

  bool get _addressesChanged {
    final contact = widget.contact;
    if (contact == null) return true;
    final before = contact.addresses.map((item) => item.address).join('\n');
    return splitLines(_addresses.text).join('\n') != before;
  }

  Future<void> _save() async {
    final contact = widget.contact;
    final name = _name.text.trim();
    final aliases = splitLines(_aliases.text);
    final addresses = splitLines(_addresses.text);
    final notes = _notes.text.trim();
    await _run(() async {
      if (contact == null) {
        await widget.api.createContact(
          name: name,
          aliases: aliases,
          addresses: addresses,
          notes: notes,
        );
      } else {
        await widget.api.updateContact(
          id: contact.id,
          name: name,
          aliases: aliases,
          notes: notes,
          addresses: _addressesChanged ? addresses : null,
        );
      }
    });
  }

  Future<void> _delete() async {
    final sure = await confirmDelete(
      context,
      title: '¿Borrar este contacto?',
      body: 'Se borran ya el contacto, sus alias y sus direcciones.',
    );
    if (!sure || !mounted) return;
    final id = widget.contact!.id;
    await _run(() => widget.api.deleteContact(id));
  }

  /// Runs one write. Success closes the editor; a failure stays on screen.
  Future<void> _run(Future<void> Function() action) async {
    setState(() {
      _busy = true;
      _failure = null;
    });
    try {
      await action();
      if (mounted) Navigator.of(context).pop();
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _failure = agendaFailureText(error);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final contact = widget.contact;
    final isNew = contact == null;
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(title: Text(isNew ? 'Nuevo contacto' : 'Contacto')),
      body: ListView(
        padding: const EdgeInsets.all(Spacing.lg),
        children: [
          _field(context, _name, 'Nombre', maxLength: 120),
          const SizedBox(height: Spacing.md),
          _field(
            context,
            _aliases,
            'Alias (uno por línea, máximo 10)',
            lines: 3,
          ),
          const SizedBox(height: Spacing.md),
          _field(
            context,
            _addresses,
            'Correos (uno por línea, máximo 10)',
            lines: 3,
          ),
          if (contact != null) ...[
            const SizedBox(height: Spacing.sm),
            for (final item in contact.addresses)
              Text(
                '${item.address}  ·  ${item.source?.label ?? ''}',
                style: context.text.small,
              ),
          ],
          const SizedBox(height: Spacing.md),
          _field(context, _notes, 'Notas', lines: 3),
          if (_failure != null) ...[
            const SizedBox(height: Spacing.md),
            Text(_failure!, style: context.text.bodySecondary),
          ],
          const SizedBox(height: Spacing.lg),
          ValueListenableBuilder<TextEditingValue>(
            valueListenable: _name,
            builder: (context, value, _) => FilledButton(
              onPressed: _busy || value.text.trim().isEmpty ? null : _save,
              child: const Text('Guardar'),
            ),
          ),
          if (contact != null) ...[
            const SizedBox(height: Spacing.md),
            ApproveGrant(
              api: widget.api,
              builder: (context, canApprove) => Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  OutlinedButton(
                    onPressed: _busy || canApprove != true ? null : _delete,
                    child: const Text('Borrar contacto'),
                  ),
                  if (canApprove == false) ...[
                    const SizedBox(height: Spacing.sm),
                    Text(approveMissingText, style: context.text.small),
                  ],
                ],
              ),
            ),
          ],
        ],
      ),
    );
  }

  Widget _field(
    BuildContext context,
    TextEditingController controller,
    String label, {
    int? maxLength,
    int lines = 1,
  }) => TextField(
    controller: controller,
    enabled: !_busy,
    maxLength: maxLength,
    minLines: lines,
    maxLines: lines,
    style: context.text.body,
    decoration: InputDecoration(labelText: label),
  );
}
