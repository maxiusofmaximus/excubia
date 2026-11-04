import 'package:flutter/material.dart';

import '../core/api/agenda_api.dart';
import '../core/api/alfred_backend_api.dart';
import '../core/api/alerts_api.dart';
import '../core/api/mail_api.dart';
import '../core/api/phone_contacts.dart';
import '../core/app_info.dart';
import '../core/state/voice_send_settings.dart';
import 'agenda/agenda_settings_section.dart';
import 'alfred/alfred_engine_section.dart';
import 'alerts/alerts_screen.dart';
import 'chat/voice_settings_screen.dart';
import 'mail/mail_settings_screen.dart';
import 'theme.dart';
import 'widgets/surface_card.dart';

/// The app menu's settings page. It holds the product identity, the agenda, the
/// mail and voice screens and the push screen, so it stays a thin map of entries.
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
  });

  final MailApi mail;
  final VoiceSendSettings voice;
  final AgendaApi agenda;
  final PhoneContacts phone;
  final AlertsApi alertRules;
  final AlfredBackendApi alfred;
  final Future<void> Function() onUnpaired;

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

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(title: const Text('Ajustes')),
      body: ListView(
        padding: const EdgeInsets.all(Spacing.lg),
        children: [
          Text('ACERCA DE', style: context.text.label),
          const SizedBox(height: Spacing.sm),
          SurfaceCard(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(appName, style: context.text.headline),
                const SizedBox(height: Spacing.xs),
                Text(appTagline, style: context.text.bodySecondary),
                const SizedBox(height: Spacing.md),
                Text('Versión $appVersion', style: context.text.small),
              ],
            ),
          ),
          AgendaSettingsSection(api: agenda, phone: phone),
          const SizedBox(height: Spacing.xl),
          Text('CORREO', style: context.text.label),
          const SizedBox(height: Spacing.sm),
          SurfaceCard(
            padding: EdgeInsets.zero,
            child: Material(
              color: Colors.transparent,
              borderRadius: BorderRadius.circular(16),
              clipBehavior: Clip.antiAlias,
              child: ListTile(
                leading: Icon(
                  Icons.mail_outline,
                  color: context.colors.accentText,
                ),
                title: const Text('Cuentas y envío'),
                subtitle: const Text('Conectar cuentas y permitir enviar'),
                trailing: const Icon(Icons.chevron_right),
                onTap: () => Navigator.of(context).push(
                  MaterialPageRoute<void>(
                    builder: (_) => MailSettingsScreen(mail: mail),
                  ),
                ),
              ),
            ),
          ),
          const SizedBox(height: Spacing.xl),
          Text('VOZ', style: context.text.label),
          const SizedBox(height: Spacing.sm),
          SurfaceCard(
            padding: EdgeInsets.zero,
            child: Material(
              color: Colors.transparent,
              borderRadius: BorderRadius.circular(16),
              clipBehavior: Clip.antiAlias,
              child: ListenableBuilder(
                listenable: voice,
                builder: (context, _) => ListTile(
                  leading: Icon(
                    Icons.mic_none,
                    color: context.colors.accentText,
                  ),
                  title: const Text('Al terminar de hablar'),
                  subtitle: Text(
                    voice.mode == VoiceSendMode.auto
                        ? 'Enviar automáticamente'
                        : 'Dejar en el cuadro para editar',
                  ),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => Navigator.of(context).push(
                    MaterialPageRoute<void>(
                      builder: (_) => VoiceSettingsScreen(voice: voice),
                    ),
                  ),
                ),
              ),
            ),
          ),
          const SizedBox(height: Spacing.xl),
          AlfredEngineSection(api: alfred),
          const SizedBox(height: Spacing.xl),
          Text('AVISOS', style: context.text.label),
          const SizedBox(height: Spacing.sm),
          SurfaceCard(
            padding: EdgeInsets.zero,
            child: Material(
              color: Colors.transparent,
              borderRadius: BorderRadius.circular(16),
              clipBehavior: Clip.antiAlias,
              child: ListTile(
                leading: Icon(
                  Icons.notifications_outlined,
                  color: context.colors.accentText,
                ),
                title: const Text('Avisos con la pantalla apagada'),
                subtitle: const Text('Batería, inicio automático y permisos'),
                trailing: const Icon(Icons.chevron_right),
                onTap: () => Navigator.of(context).push(
                  MaterialPageRoute<void>(
                    builder: (_) => AlertsScreen(rules: alertRules),
                  ),
                ),
              ),
            ),
          ),
          const SizedBox(height: Spacing.xl),
          Text('EMPAREJAMIENTO', style: context.text.label),
          const SizedBox(height: Spacing.sm),
          SurfaceCard(
            padding: EdgeInsets.zero,
            child: Material(
              color: Colors.transparent,
              borderRadius: BorderRadius.circular(16),
              clipBehavior: Clip.antiAlias,
              child: ListTile(
                leading: Icon(
                  Icons.link_outlined,
                  color: context.colors.accentText,
                ),
                title: const Text('Emparejar de nuevo'),
                trailing: const Icon(Icons.chevron_right),
                onTap: () => _confirmRepair(context),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
