import 'package:flutter/material.dart';

import '../../core/api/agenda_api.dart';
import '../../core/api/phone_contacts.dart';
import '../../core/state/contacts_import_controller.dart';
import '../theme.dart';
import '../widgets/surface_card.dart';
import 'agenda_format.dart';
import 'agenda_widgets.dart';

/// The explanation comes before the system's question, and says what is read,
/// where it goes and what it never changes. A denial is shown with a way out.
class AgendaImportScreen extends StatefulWidget {
  const AgendaImportScreen({super.key, required this.api, required this.phone});

  final AgendaApi api;
  final PhoneContacts phone;

  @override
  State<AgendaImportScreen> createState() => _AgendaImportScreenState();
}

class _AgendaImportScreenState extends State<AgendaImportScreen> {
  late final ContactsImportController _import = ContactsImportController(
    phone: widget.phone,
    api: widget.api,
  );

  @override
  void dispose() {
    _import.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(title: const Text('Importar contactos')),
      body: ListView(
        padding: const EdgeInsets.all(Spacing.lg),
        children: [
          SurfaceCard(
            child: Text(
              'Leeré el nombre y los correos de los contactos de tu teléfono. '
              'Se envían al PC de esta app, no a terceros. Solo se añaden '
              'direcciones nuevas: no se cambia ningún nombre, alias ni nota, y '
              'no se borra nada. Puedes retirar el permiso en los ajustes del sistema.',
              style: context.text.body,
            ),
          ),
          const SizedBox(height: Spacing.lg),
          ApproveGrant(
            api: widget.api,
            builder: (context, canApprove) => ListenableBuilder(
              listenable: _import,
              builder: (context, _) => _actions(context, canApprove),
            ),
          ),
        ],
      ),
    );
  }

  Widget _actions(BuildContext context, bool? canApprove) {
    final phase = _import.phase;
    final running = phase == ContactsImportPhase.running;
    final report = _import.report;
    final again = phase == ContactsImportPhase.done;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        FilledButton(
          onPressed: running || canApprove != true ? null : _import.run,
          child: Text(
            again ? 'Repetir la importación' : 'Permitir y leer los contactos',
          ),
        ),
        if (canApprove == false) ...[
          const SizedBox(height: Spacing.sm),
          Text(approveMissingText, style: context.text.small),
        ],
        const SizedBox(height: Spacing.lg),
        if (running)
          const Center(child: CircularProgressIndicator())
        else if (report != null)
          SurfaceCard(
            child: Text(importSummary(report), style: context.text.body),
          )
        else if (phase == ContactsImportPhase.denied) ...[
          Text(
            'No diste permiso, así que no se ha leído nada. Puedes intentarlo otra vez.',
            style: context.text.bodySecondary,
          ),
          const SizedBox(height: Spacing.md),
          _settingsButton(),
        ] else if (phase == ContactsImportPhase.deniedForever) ...[
          Text(
            'El permiso está denegado. Para darlo, abre los ajustes de la app y actívalo allí.',
            style: context.text.bodySecondary,
          ),
          const SizedBox(height: Spacing.md),
          _settingsButton(),
        ] else if (phase == ContactsImportPhase.failed)
          Text(_import.failure ?? '', style: context.text.bodySecondary),
      ],
    );
  }

  Widget _settingsButton() => OutlinedButton(
    onPressed: widget.phone.openAppSettings,
    child: const Text('Abrir ajustes de la app'),
  );
}
