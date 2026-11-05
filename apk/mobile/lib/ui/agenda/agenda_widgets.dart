import 'package:flutter/material.dart';

import '../../core/api/agenda_api.dart';
import '../theme.dart';

/// A short Spanish message with an optional action, for empty and failed states.
class AgendaMessage extends StatelessWidget {
  const AgendaMessage({
    super.key,
    required this.text,
    this.actionLabel,
    this.onAction,
  });

  final String text;
  final String? actionLabel;
  final VoidCallback? onAction;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.all(Spacing.lg),
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          text,
          style: context.text.bodySecondary,
          textAlign: TextAlign.center,
        ),
        if (actionLabel != null && onAction != null) ...[
          const SizedBox(height: Spacing.md),
          OutlinedButton(onPressed: onAction, child: Text(actionLabel!)),
        ],
      ],
    ),
  );
}

/// The origin of an address, in small text. Nothing is shown for an unknown one.
class SourceTag extends StatelessWidget {
  const SourceTag({super.key, required this.source});

  final AgendaSource? source;

  @override
  Widget build(BuildContext context) {
    final tag = source;
    if (tag == null) return const SizedBox.shrink();
    return Text(
      tag.label,
      style: context.text.small.copyWith(color: context.colors.textMuted),
    );
  }
}

/// Hands [builder] whether this device holds `APPROVE`: `null` while the host is
/// asked, `false` when it cannot be asked. The question is asked once per screen.
class ApproveGrant extends StatefulWidget {
  const ApproveGrant({super.key, required this.api, required this.builder});

  final AgendaApi api;
  final Widget Function(BuildContext context, bool? canApprove) builder;

  @override
  State<ApproveGrant> createState() => _ApproveGrantState();
}

class _ApproveGrantState extends State<ApproveGrant> {
  late final Future<bool> _grant = widget.api.canApprove();

  @override
  Widget build(BuildContext context) => FutureBuilder<bool>(
    future: _grant,
    builder: (context, snapshot) =>
        widget.builder(context, snapshot.hasError ? false : snapshot.data),
  );
}

/// The reason a delete or an import is off, said in full. Shown under the button.
const String approveMissingText =
    'Este dispositivo no tiene permiso para aprobar, así que no puede borrar ni importar.';

/// Asks before a delete that cannot be undone. Answers `true` only on «Borrar».
Future<bool> confirmDelete(
  BuildContext context, {
  required String title,
  required String body,
}) async {
  final answer = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(title),
      content: Text(body),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: const Text('Cancelar'),
        ),
        FilledButton(
          onPressed: () => Navigator.of(context).pop(true),
          child: const Text('Borrar'),
        ),
      ],
    ),
  );
  return answer ?? false;
}
