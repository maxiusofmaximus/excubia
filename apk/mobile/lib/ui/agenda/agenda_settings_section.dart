import 'package:flutter/material.dart';

import '../../core/api/agenda_api.dart';
import '../../core/api/phone_contacts.dart';
import '../theme.dart';
import '../widgets/surface_card.dart';
import 'agenda_contacts_screen.dart';
import 'agenda_notes_screen.dart';
import 'calendar_screen.dart';

/// The agenda's screens, as entries of the settings page.
class AgendaSettingsSection extends StatelessWidget {
  const AgendaSettingsSection({
    super.key,
    required this.api,
    required this.phone,
  });

  final AgendaApi api;
  final PhoneContacts phone;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text('AGENDA', style: context.text.label),
      const SizedBox(height: Spacing.sm),
      SurfaceCard(
        padding: EdgeInsets.zero,
        child: Column(
          children: [
            _entry(
              context,
              icon: Icons.contacts_outlined,
              title: 'Contactos',
              subtitle: 'Personas, alias y correos',
              screen: AgendaContactsScreen(api: api, phone: phone),
            ),
            const Divider(height: 1),
            _entry(
              context,
              icon: Icons.sticky_note_2_outlined,
              title: 'Notas',
              subtitle: 'Notas guardadas en el equipo',
              screen: AgendaNotesScreen(api: api),
            ),
            const Divider(height: 1),
            _entry(
              context,
              icon: Icons.event_outlined,
              title: 'Calendario',
              subtitle: 'Eventos y avisos, en la hora de la empresa',
              screen: CalendarScreen(api: api),
            ),
          ],
        ),
      ),
    ],
  );

  Widget _entry(
    BuildContext context, {
    required IconData icon,
    required String title,
    required String subtitle,
    required Widget screen,
  }) => Material(
    color: Colors.transparent,
    child: ListTile(
      leading: Icon(icon, color: context.colors.accentText),
      title: Text(title),
      subtitle: Text(subtitle),
      trailing: const Icon(Icons.chevron_right),
      onTap: () =>
          Navigator.of(context)
              .push(MaterialPageRoute<void>(builder: (_) => screen)),
    ),
  );
}
