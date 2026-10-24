import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/approvals/model.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';

import '../../core/state/approvals_controller.dart';
import '../theme.dart';
import 'approval_card.dart';

/// Ajustes → Aprobaciones: the global mode, one override per project, and the
/// approvals waiting for this user (the secondary inbox opens the same card).
class ApprovalSettingsScreen extends StatefulWidget {
  const ApprovalSettingsScreen({
    super.key,
    required this.approvals,
    required this.conversationId,
    required this.projects,
  });

  final ApprovalsController approvals;
  final String? conversationId;
  final List<ChatProject> projects;

  @override
  State<ApprovalSettingsScreen> createState() => _ApprovalSettingsScreenState();
}

class _ApprovalSettingsScreenState extends State<ApprovalSettingsScreen> {
  @override
  void initState() {
    super.initState();
    widget.approvals.loadSettings();
  }

  @override
  Widget build(BuildContext context) {
    final approvals = widget.approvals;
    final text = context.text;
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(title: const Text('Aprobaciones')),
      body: ListenableBuilder(
        listenable: approvals,
        builder: (context, _) {
          final settings = approvals.settings;
          final denied = approvals.settingsDenied;
          final locked = denied || settings == null;
          final pending = approvals.userPending();
          return ListView(
            padding: const EdgeInsets.all(Spacing.lg),
            children: [
              if (denied)
                _DeniedNotice(command: approvals.settingsFailure?.command ?? '')
              else if (approvals.settingsFailure != null)
                Text(
                  'No se pudieron leer los ajustes. Vuelve a intentarlo.',
                  style: text.bodySecondary,
                ),
              SwitchListTile(
                contentPadding: EdgeInsets.zero,
                title: const Text('Pedirme siempre a mí'),
                value: settings?.global == 'always_user',
                onChanged: locked
                    ? null
                    : (value) => _setAlwaysUser(context, approvals, value),
              ),
              Text(
                'Con el ajuste activo, cada permiso llega a ti; sin él, el lead y Alfred deciden antes.',
                style: text.small,
              ),
              const SizedBox(height: Spacing.lg),
              Text('Excepciones por proyecto', style: text.label),
              const SizedBox(height: Spacing.xs),
              if (widget.projects.isEmpty)
                Text('Aún no hay proyectos.', style: text.bodySecondary),
              for (final project in widget.projects)
                _ProjectSwitch(
                  name: project.name,
                  ownMode: _overrideFor(settings, project.id),
                  globalMode: settings?.global,
                  onChanged: locked
                      ? null
                      : (value) => approvals.setMode(
                          mode: value,
                          projectId: project.id,
                        ),
                ),
              if (pending.isNotEmpty) ...[
                const SizedBox(height: Spacing.lg),
                Text('Pendientes de ti', style: text.label),
                const SizedBox(height: Spacing.sm),
                for (final approval in pending)
                  Padding(
                    padding: const EdgeInsets.only(bottom: Spacing.sm),
                    child: ApprovalCardHost(
                      controller: approvals,
                      approvalId: approval.approvalId,
                      projects: widget.projects,
                    ),
                  ),
              ],
            ],
          );
        },
      ),
    );
  }

  /// The project's own mode, or null when it inherits the global one.
  String? _overrideFor(ApprovalSettings? settings, String projectId) {
    if (settings == null) return null;
    for (final row in settings.projects) {
      if (row.projectId == projectId) return row.mode;
    }
    return null;
  }

  /// Turning the switch off hands the first decision to the lead and Alfred, so
  /// it asks first and says so in one line.
  Future<void> _setAlwaysUser(
    BuildContext context,
    ApprovalsController approvals,
    bool alwaysUser,
  ) async {
    if (!alwaysUser) {
      final confirmed = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          content: const Text(
            'Al apagarlo, el lead y Alfred deciden antes; a ti solo te llegan los permisos que escalen.',
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('Cancelar'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: const Text('Apagar'),
            ),
          ],
        ),
      );
      if (confirmed != true) return;
    }
    await approvals.setMode(mode: alwaysUser ? 'always_user' : 'delegated');
  }
}

class _ProjectSwitch extends StatelessWidget {
  const _ProjectSwitch({
    required this.name,
    required this.ownMode,
    required this.globalMode,
    required this.onChanged,
  });

  final String name;
  final String? ownMode;
  final String? globalMode;
  final ValueChanged<String>? onChanged;

  @override
  Widget build(BuildContext context) {
    final effective = ownMode ?? globalMode;
    return SwitchListTile(
      contentPadding: EdgeInsets.zero,
      title: Text(name),
      subtitle: Text(
        ownMode == null ? 'Usa el ajuste global' : 'Ajuste propio',
      ),
      value: effective == 'always_user',
      onChanged: onChanged == null
          ? null
          : (value) => onChanged!(value ? 'always_user' : 'delegated'),
    );
  }
}

/// The 403 without `APPROVE`: the controls stay off and the host line says why.
class _DeniedNotice extends StatelessWidget {
  const _DeniedNotice({required this.command});

  final String command;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final text = context.text;
    return Padding(
      padding: const EdgeInsets.only(bottom: Spacing.lg),
      child: Container(
        padding: const EdgeInsets.all(Spacing.md),
        decoration: BoxDecoration(
          color: colors.raised,
          borderRadius: BorderRadius.circular(12),
          border: Border.all(color: colors.outlineSoft),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Este dispositivo no tiene el permiso APPROVE, así que no puede cambiar la aprobación. En el host, ejecuta:',
              style: text.bodySecondary,
            ),
            const SizedBox(height: Spacing.sm),
            SelectableText(command, style: text.mono),
          ],
        ),
      ),
    );
  }
}
