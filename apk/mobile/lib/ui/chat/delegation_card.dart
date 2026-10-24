import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';

import '../theme.dart';
import '../widgets/surface_card.dart';

/// Some host rows arrive with UTF-8 bytes read as Latin-1 ("Ã³" instead of
/// "ó"). Repair only that exact signature; leave normal text untouched.
String normalizeCardText(String value) {
  if (!value.contains('Ã') && !value.contains('Â')) {
    return value;
  }
  try {
    return utf8.decode(latin1.encode(value));
  } on FormatException {
    return value;
  } on ArgumentError {
    return value;
  }
}

/// Typed delegation states from `contracts/api.md`. The host still sends
/// `done`; `completed` is accepted for older rows.
const Set<String> terminalDelegationStates = {
  'done',
  'completed',
  'failed',
  'cancelled',
  'interrupted',
};

bool delegationIsActive(String? status) =>
    status != null && !terminalDelegationStates.contains(status);

String delegationStatusLabel(String? status) => switch (status) {
  'queued' => 'En cola',
  'running' => 'En curso',
  'waiting' => 'Esperando',
  'done' || 'completed' => 'Completada',
  'failed' => 'Falló',
  'cancelled' => 'Cancelada',
  'interrupted' => 'Interrumpida',
  _ => 'Desconocida',
};

Color delegationStatusColor(BuildContext context, String? status) {
  final c = context.colors;
  return switch (status) {
    'running' => c.statusWorking,
    'waiting' => c.statusBlocked,
    'done' || 'completed' => c.statusDone,
    'failed' => c.statusBlocked,
    'cancelled' || 'interrupted' => c.textMuted,
    _ => c.textSecondary,
  };
}

IconData delegationStatusIcon(String? status) => switch (status) {
  'queued' => Icons.schedule,
  'running' => Icons.autorenew,
  'waiting' => Icons.pan_tool_outlined,
  'done' || 'completed' => Icons.check_circle_outline,
  'failed' => Icons.error_outline,
  'cancelled' => Icons.cancel_outlined,
  'interrupted' => Icons.link_off,
  _ => Icons.help_outline,
};

/// Ask before stopping a live delegation. The same wording is used from the
/// card and from the detail screen so the action is predictable.
Future<bool> confirmDelegationCancel(
  BuildContext context, {
  required String title,
}) async {
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: const Text('¿Cancelar la delegación?'),
      content: Text(
        '$title dejará de trabajar. La tarjeta y el historial se conservan, '
        'y la delegación quedará como cancelada.',
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: const Text('Volver'),
        ),
        FilledButton(
          onPressed: () => Navigator.of(context).pop(true),
          style: FilledButton.styleFrom(
            backgroundColor: context.colors.statusBlocked,
            foregroundColor: context.colors.onAccent,
          ),
          child: const Text('Cancelar delegación'),
        ),
      ],
    ),
  );
  return confirmed ?? false;
}

/// One `kind=card` timeline entry. Flutter only paints what Rust reduced:
/// title, live status, one-line summary, pane and delegation links.
class DelegationCard extends StatelessWidget {
  const DelegationCard({
    super.key,
    required this.entry,
    required this.onOpenAgent,
    required this.onOpenDetail,
    required this.onCancel,
  });

  final ChatEntry entry;
  final ValueChanged<ChatEntry> onOpenAgent;
  final ValueChanged<ChatEntry> onOpenDetail;
  final ValueChanged<ChatEntry> onCancel;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final status = entry.status ?? entry.state ?? 'queued';
    final active = delegationIsActive(status);
    final color = delegationStatusColor(context, status);
    final summary = normalizeCardText(entry.summary?.trim() ?? '');
    final title = normalizeCardText(
      (entry.title?.trim().isNotEmpty ?? false)
          ? entry.title!.trim()
          : 'Delegación del equipo',
    );
    final paneId = entry.paneId;
    return Semantics(
      container: true,
      label: 'Delegación: $title, ${delegationStatusLabel(status)}',
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: Spacing.lg,
          vertical: Spacing.xs,
        ),
        child: SurfaceCard(
          padding: const EdgeInsets.fromLTRB(
            Spacing.md,
            Spacing.md,
            Spacing.md,
            Spacing.sm,
          ),
          borderColor: active ? color.withValues(alpha: 0.55) : c.outlineSoft,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(Icons.account_tree_outlined, size: 20, color: color),
                  const SizedBox(width: Spacing.sm),
                  Expanded(child: Text(title, style: context.text.title)),
                  const SizedBox(width: Spacing.sm),
                  _StatusPill(status: status, color: color),
                ],
              ),
              if (summary.isNotEmpty) ...[
                const SizedBox(height: Spacing.xs),
                Text(
                  summary,
                  style: context.text.bodySecondary,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                ),
              ],
              if (active) ...[
                const SizedBox(height: Spacing.sm),
                Semantics(
                  label: 'Delegación en curso',
                  child: ClipRRect(
                    borderRadius: BorderRadius.circular(2),
                    child: LinearProgressIndicator(
                      minHeight: 2,
                      color: color,
                      backgroundColor: c.raised,
                    ),
                  ),
                ),
              ],
              if ((entry.attention ?? 0) > 0) ...[
                const SizedBox(height: Spacing.xs),
                Row(
                  children: [
                    Icon(
                      Icons.notifications_active_outlined,
                      size: 16,
                      color: c.statusBlocked,
                    ),
                    const SizedBox(width: Spacing.xs),
                    Text(
                      'Requiere tu atención',
                      style: context.text.small.copyWith(
                        color: c.statusBlocked,
                      ),
                    ),
                  ],
                ),
              ],
              const SizedBox(height: Spacing.xs),
              Wrap(
                spacing: Spacing.xs,
                runSpacing: 0,
                children: [
                  if (paneId != null && paneId.isNotEmpty)
                    TextButton.icon(
                      onPressed: () => onOpenAgent(entry),
                      style: TextButton.styleFrom(
                        minimumSize: const Size(48, 48),
                      ),
                      icon: const Icon(Icons.terminal_outlined, size: 18),
                      label: const Text('Abrir agente'),
                    ),
                  TextButton.icon(
                    onPressed: () => onOpenDetail(entry),
                    style: TextButton.styleFrom(
                      minimumSize: const Size(48, 48),
                    ),
                    icon: const Icon(Icons.open_in_new, size: 18),
                    label: const Text('Ver resultado completo'),
                  ),
                  if (active)
                    TextButton.icon(
                      onPressed: () => onCancel(entry),
                      style: TextButton.styleFrom(
                        minimumSize: const Size(48, 48),
                        foregroundColor: c.statusBlocked,
                      ),
                      icon: const Icon(Icons.stop_circle_outlined, size: 18),
                      label: const Text('Cancelar'),
                    ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _StatusPill extends StatelessWidget {
  const _StatusPill({required this.status, required this.color});

  final String status;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      constraints: const BoxConstraints(minHeight: 26),
      padding: const EdgeInsets.symmetric(horizontal: Spacing.sm, vertical: 4),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(999),
        border: Border.all(color: color.withValues(alpha: 0.35)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(delegationStatusIcon(status), size: 14, color: color),
          const SizedBox(width: 4),
          Text(
            delegationStatusLabel(status),
            style: context.text.label.copyWith(color: color, fontSize: 11),
          ),
        ],
      ),
    );
  }
}
