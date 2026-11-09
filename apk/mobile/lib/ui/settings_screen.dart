import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';

import '../core/api/agenda_api.dart';
import '../core/api/alfred_backend_api.dart';
import '../core/api/alerts_api.dart';
import '../core/api/mail_api.dart';
import '../core/api/phone_contacts.dart';
import '../core/app_info.dart';
import '../core/state/approvals_controller.dart';
import '../core/state/voice_send_settings.dart';
import 'agenda/agenda_settings_section.dart';
import 'alfred/alfred_engine_section.dart';
import 'alerts/alerts_screen.dart';
import 'chat/approval_entry.dart';
import 'chat/voice_settings_screen.dart';
import 'mail/mail_settings_screen.dart';
import 'new_conversation_screen.dart' show CatalogLoader;
import 'palette/palette_screen.dart';
import 'theme.dart';
import 'widgets/grouped_card.dart';

/// The app menu's settings page: one grouped card per topic, in the order of
/// the approved mockup. The agenda and Alfred's engine are their own sections.
class SettingsScreen extends StatelessWidget {
  const SettingsScreen({
    super.key,
    required this.mail,
    required this.voice,
    required this.agenda,
    required this.phone,
    required this.alertRules,
    required this.alfred,
    required this.onUnpaired,
    this.approvals,
    this.conversationId,
    this.projects = const <ChatProject>[],
    this.alfredSeat,
    this.catalog,
    this.setDefaultAgent,
  });

  final MailApi mail;
  final VoiceSendSettings voice;
  final AgendaApi agenda;
  final PhoneContacts phone;
  final AlertsApi alertRules;
  final AlfredBackendApi alfred;
  final Future<void> Function() onUnpaired;

  /// The approvals of the open chat. Without them the Aprobaciones row is hidden.
  final ApprovalsController? approvals;
  final String? conversationId;
  final List<ChatProject> projects;
  final ChatSeat? alfredSeat;

  /// The host's agent catalogue, for the installed-agents list under Alfred's engine.
  final CatalogLoader? catalog;

  /// Makes an installed agent the PC's default; the Alfred section offers it.
  final DefaultAgentSetter? setDefaultAgent;

  Future<void> _confirmRepair(BuildContext context) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialog) => AlertDialog(
        title: const Text('¿Emparejar de nuevo?'),
        content: const Text(
          'Este teléfono olvidará su emparejamiento actual y te pedirá un enlace nuevo. '
          'El dispositivo actual seguirá siendo válido en el PC hasta que lo revoques allí.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialog).pop(false),
            child: const Text('Cancelar'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialog).pop(true),
            child: const Text('Emparejar de nuevo'),
          ),
        ],
      ),
    );
    if (confirmed != true || !context.mounted) return;
    // Back to the root first, so the pairing screen is not hidden behind Ajustes.
    Navigator.of(context).popUntil((route) => route.isFirst);
    await onUnpaired();
  }

  void _push(BuildContext context, Widget Function() page) {
    Navigator.of(context).push(MaterialPageRoute<void>(builder: (_) => page()));
  }

  @override
  Widget build(BuildContext context) {
    final approvals = this.approvals;
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(title: const Text('Ajustes')),
      body: ListView(
        padding: const EdgeInsets.all(Spacing.lg),
        children: [
          _SettingsSection(
            label: 'APARIENCIA',
            children: [
              _SettingsEntry(
                icon: Icons.palette_outlined,
                title: 'Paleta de colores',
                subtitle: 'Tema claro u oscuro, y el color de la app',
                onTap: () => _push(
                  context,
                  () => const PaletteScreen(mode: PaletteScreenMode.settings),
                ),
              ),
            ],
          ),
          const SizedBox(height: Spacing.xl),
          AgendaSettingsSection(api: agenda, phone: phone),
          const SizedBox(height: Spacing.xl),
          AlfredEngineSection(
            api: alfred,
            catalog: catalog,
            setDefault: setDefaultAgent,
          ),
          const SizedBox(height: Spacing.xl),
          if (approvals != null) ...[
            _SettingsSection(
              label: 'APROBACIONES',
              children: [
                _SettingsEntry(
                  icon: Icons.shield_outlined,
                  title: 'Aprobaciones',
                  onTap: () => openApprovalSettings(
                    context,
                    approvals: approvals,
                    conversationId: conversationId,
                    projects: projects,
                    alfred: alfredSeat,
                  ),
                ),
              ],
            ),
            const SizedBox(height: Spacing.xl),
          ],
          _SettingsSection(
            label: 'VOZ',
            children: [
              ListenableBuilder(
                listenable: voice,
                builder: (context, _) => _SettingsEntry(
                  icon: Icons.mic_none,
                  title: 'Al terminar de hablar',
                  subtitle: voice.mode == VoiceSendMode.auto
                      ? 'Enviar automáticamente'
                      : 'Dejar en el cuadro para editar',
                  onTap: () =>
                      _push(context, () => VoiceSettingsScreen(voice: voice)),
                ),
              ),
            ],
          ),
          const SizedBox(height: Spacing.xl),
          _SettingsSection(
            label: 'CORREO',
            children: [
              _SettingsEntry(
                icon: Icons.mail_outline,
                title: 'Cuentas y envío',
                subtitle: 'Conectar cuentas y permitir enviar',
                onTap: () =>
                    _push(context, () => MailSettingsScreen(mail: mail)),
              ),
            ],
          ),
          const SizedBox(height: Spacing.xl),
          _SettingsSection(
            label: 'AVISOS',
            children: [
              _SettingsEntry(
                icon: Icons.notifications_outlined,
                title: 'Avisos con la pantalla apagada',
                subtitle: 'Batería, inicio automático y permisos',
                onTap: () =>
                    _push(context, () => AlertsScreen(rules: alertRules)),
              ),
            ],
          ),
          const SizedBox(height: Spacing.xl),
          _SettingsSection(
            label: 'ESTE TELÉFONO',
            children: [
              _SettingsEntry(
                icon: Icons.link_outlined,
                title: 'Emparejar de nuevo',
                onTap: () => _confirmRepair(context),
              ),
            ],
          ),
          const SizedBox(height: Spacing.xl),
          _SettingsSection(
            label: 'ACERCA DE',
            children: [
              Padding(
                padding: const EdgeInsets.all(Spacing.lg),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(appName, style: context.text.title),
                    const SizedBox(height: Spacing.xs),
                    Text(appTagline, style: context.text.bodySecondary),
                    const SizedBox(height: Spacing.md),
                    Text('Versión $appVersion', style: context.text.small),
                  ],
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// A topic's label above one card whose rows are separated by hairlines.
class _SettingsSection extends StatelessWidget {
  const _SettingsSection({required this.label, required this.children});

  final String label;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      Text(label, style: context.text.label),
      const SizedBox(height: Spacing.sm),
      GroupedCard(children: children),
    ],
  );
}

class _SettingsEntry extends StatelessWidget {
  const _SettingsEntry({
    required this.icon,
    required this.title,
    required this.onTap,
    this.subtitle,
  });

  final IconData icon;
  final String title;
  final String? subtitle;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => ListTile(
    minTileHeight: 56,
    leading: Icon(icon, color: context.colors.accentText),
    title: Text(title),
    subtitle: subtitle == null ? null : Text(subtitle!),
    trailing: const Icon(Icons.chevron_right),
    onTap: onTap,
  );
}
