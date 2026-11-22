import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:excubia/src/rust/api/chat/model.dart';

import '../theme.dart';
import '../widgets/surface_card.dart';
import 'markdown_text.dart';

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

/// Text takes the status colour, except blocked states: coral is only a fill or
/// an icon, so waiting and failed read in neutral ink.
Color delegationStatusTextColor(BuildContext context, String? status) {
  final c = context.colors;
  return switch (status) {
    'waiting' || 'failed' => c.textPrimary,
    _ => delegationStatusColor(context, status),
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

/// The agent's name from the host's title «Alfred pidió a Ernesto»; null when
/// the title has none.
String? delegateName(String title) =>
    RegExp(r' (?:pidió a|asignó a) (.+)$').firstMatch(title)?.group(1);

/// The card's one line: what the agent does with the request, in plain words.
String delegationLine(String? agent, String? status, {bool attention = false}) {
  final who = agent ?? 'Un agente';
  if (attention && delegationIsActive(status)) {
    return '$who necesita tu atención';
  }
  return switch (status) {
    'done' || 'completed' => '$who terminó',
    'failed' => '$who falló',
    'cancelled' => 'Delegación cancelada',
    'interrupted' => '$who se interrumpió',
    'waiting' => '$who está esperando',
    _ => '$who está trabajando en tu pedido',
  };
}

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
            foregroundColor: context.colors.onAttention,
          ),
          child: const Text('Cancelar delegación'),
        ),
      ],
    ),
  );
  return confirmed ?? false;
}

/// One `kind=card` timeline entry: one line of what the agent does, a live bar
/// while it works, and «Ver detalle». The agent, the result and the cancel
/// button live on the detail screen.
class DelegationCard extends StatelessWidget {
  const DelegationCard({
    super.key,
    required this.entry,
    required this.onOpenDetail,
  });

  final ChatEntry entry;
  final ValueChanged<ChatEntry> onOpenDetail;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final status = entry.status ?? entry.state ?? 'queued';
    final active = delegationIsActive(status);
    final color = delegationStatusColor(context, status);
    final title = plainFromMarkdown(
      normalizeCardText(
        (entry.title?.trim().isNotEmpty ?? false)
            ? entry.title!.trim()
            : 'Delegación del equipo',
      ),
    );
    final line = delegationLine(
      delegateName(title),
      status,
      attention: (entry.attention ?? 0) > 0,
    );
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
            Spacing.sm,
            Spacing.sm,
            Spacing.sm,
          ),
          borderColor: active ? color.withValues(alpha: 0.55) : c.outlineSoft,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(Icons.account_tree_outlined, size: 20, color: color),
                  const SizedBox(width: Spacing.sm),
                  Expanded(
                    child: Text(
                      line,
                      style: context.text.title,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  TextButton(
                    onPressed: () => onOpenDetail(entry),
                    style: TextButton.styleFrom(
                      minimumSize: const Size(48, 48),
                    ),
                    child: const Text('Ver detalle'),
                  ),
                ],
              ),
              if (active) ...[
                const SizedBox(height: Spacing.xs),
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
            ],
          ),
        ),
      ),
    );
  }
}
