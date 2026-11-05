import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../core/alerts/alert_platform.dart';
import '../../core/api/alerts_api.dart';
import '../../core/alerts/alert_status.dart';
import '../../core/alerts/manufacturer_guide.dart';
import '../theme.dart';
import 'alert_rules_screen.dart';

const Map<String, String> _channelNames = {
  'agents': 'Aprobaciones y preguntas',
  'agents-review': 'Tareas terminadas',
  'recordatorios': 'Recordatorios del calendario',
  'conexion': 'Conexión con el PC',
};

/// The preparation rows of the alerts service (RFC-PUSH-PROPIO §6, F0). It reads the phone
/// again whenever the app comes back to the front, and changes nothing on its own.
class AlertsSetupSection extends StatefulWidget {
  const AlertsSetupSection({
    super.key,
    required this.platform,
    required this.rules,
  });

  final AlertPlatform platform;
  final AlertsApi rules;

  @override
  State<AlertsSetupSection> createState() => _AlertsSetupSectionState();
}

class _AlertsSetupSectionState extends State<AlertsSetupSection>
    with WidgetsBindingObserver {
  AlertStatus _status = AlertStatus.unknown;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _reload();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) _reload();
  }

  Future<void> _reload() async {
    final status = await widget.platform.status();
    if (mounted) setState(() => _status = status);
  }

  Future<void> _act(Future<void> Function() action) async {
    await action();
    await _reload();
  }

  @override
  Widget build(BuildContext context) {
    final text = context.text;
    final status = _status;
    final steps = manufacturerSteps(status.manufacturer);
    final running = status.serviceRunning ? 'En marcha' : 'Detenido';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('Lo que necesita tu móvil', style: text.label),
        const SizedBox(height: Spacing.sm),
        _Requirement(
          title: 'Permiso de notificaciones',
          value: status.notificationsGranted ? 'Concedido' : 'Denegado',
          button: status.notificationsGranted ? null : 'Permitir',
          onPressed: () => _act(widget.platform.requestNotificationPermission),
        ),
        const SizedBox(height: Spacing.md),
        Text('Canales', style: text.label),
        const SizedBox(height: Spacing.xs),
        for (final entry in status.channels.entries)
          Row(
            children: [
              Expanded(
                child: Text(
                  '${_channelNames[entry.key] ?? entry.key}: '
                  '${entry.value ? 'activo' : 'desactivado'}',
                  style: text.body,
                ),
              ),
              TextButton(
                onPressed: () => widget.platform.openChannelSettings(entry.key),
                child: const Text('Ajustes'),
              ),
            ],
          ),
        const SizedBox(height: Spacing.md),
        _Requirement(
          title: 'Batería',
          value: status.batteryExempt
              ? 'Sin restricciones'
              : 'Optimizada: Android puede parar los avisos',
          button: 'Abrir ajustes de batería',
          onPressed: widget.platform.openBatterySettings,
        ),
        const SizedBox(height: Spacing.md),
        if (status.bootMissed) ...[
          Text(
            'Tras el último reinicio los avisos no volvieron solos: '
            'activa el inicio automático de esta app.',
            style: text.body.copyWith(color: context.colors.statusBlocked),
          ),
          const SizedBox(height: Spacing.md),
        ],
        _Requirement(
          title: 'Inicio automático',
          value: status.autostartScreen
              ? 'Actívalo para esta app: así los avisos vuelven solos tras reiniciar el móvil. '
                    'Android no deja a la app comprobarlo.'
              : 'Android no deja a la app comprobarlo. '
                    'Busca «Inicio automático» en los ajustes de esta app.',
          button: status.autostartScreen
              ? 'Abrir inicio automático'
              : 'Abrir ajustes de esta app',
          onPressed: status.autostartScreen
              ? () => widget.platform.openAutostartSettings()
              : () => widget.platform.openAppSettings(),
        ),
        const SizedBox(height: Spacing.md),
        SwitchListTile(
          contentPadding: EdgeInsets.zero,
          title: const Text('Avisos en segundo plano'),
          subtitle: Text(running, style: text.bodySecondary),
          value: status.enabled,
          onChanged: (on) => _act(() => widget.platform.setEnabled(on)),
        ),
        const SizedBox(height: Spacing.xs),
        Text('Última parada: ${status.lastExit}', style: text.bodySecondary),
        const SizedBox(height: Spacing.xs),
        Text(
          'Si fuerzas la detención de la app desde los ajustes de Android, los avisos '
          'se paran hasta que la abras de nuevo. Tras reiniciar el móvil, Android solo '
          'los reanuda cuando lo desbloqueas.',
          style: text.bodySecondary,
        ),
        if (steps != null) ...[
          const SizedBox(height: Spacing.md),
          Text(
            'Pasos en el móvil (pueden variar según la versión)',
            style: text.label,
          ),
          const SizedBox(height: Spacing.xs),
          for (final step in steps) Text(step, style: text.bodySecondary),
        ],
        const SizedBox(height: Spacing.md),
        OutlinedButton(
          onPressed: () => Navigator.of(context).push(
            MaterialPageRoute<void>(
              builder: (_) => AlertRulesScreen(api: widget.rules),
            ),
          ),
          child: const Text('Reglas de aviso'),
        ),
        const SizedBox(height: Spacing.sm),
        OutlinedButton(
          onPressed: _copyDiagnostics,
          child: const Text('Copiar diagnóstico'),
        ),
      ],
    );
  }

  Future<void> _copyDiagnostics() async {
    final log = await widget.platform.readLog();
    await Clipboard.setData(ClipboardData(text: log));
    if (mounted) {
      ScaffoldMessenger.of(context)
          .showSnackBar(const SnackBar(content: Text('Diagnóstico copiado')));
    }
  }
}

class _Requirement extends StatelessWidget {
  const _Requirement({
    required this.title,
    required this.value,
    required this.onPressed,
    this.button,
  });

  final String title;
  final String value;
  final String? button;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final text = context.text;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(title, style: text.label),
        const SizedBox(height: Spacing.xs),
        Text(value, style: text.body),
        if (button != null) ...[
          const SizedBox(height: Spacing.xs),
          OutlinedButton(onPressed: onPressed, child: Text(button!)),
        ],
      ],
    );
  }
}
