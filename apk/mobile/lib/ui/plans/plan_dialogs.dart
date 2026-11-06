import 'package:flutter/material.dart';

import '../../core/api/plan_models.dart';
import '../theme.dart';
import 'plan_text.dart';

/// Asks once more before a plan that touches mail or the internet is approved.
Future<bool> confirmApprovePlan(BuildContext context) async =>
    await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        content: const Text(planConfirmApproveText),
        actions: [
          TextButton(
            style: TextButton.styleFrom(minimumSize: const Size(48, 48)),
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Volver'),
          ),
          FilledButton(
            style: FilledButton.styleFrom(minimumSize: const Size(48, 48)),
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Aprobar igualmente'),
          ),
        ],
      ),
    ) ??
    false;

/// Asks before a plan is rejected. Nothing of it is done either way.
Future<bool> confirmRejectPlan(BuildContext context) async =>
    await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text(planRejectTitle),
        content: const Text(planRejectDetail),
        actions: [
          TextButton(
            style: TextButton.styleFrom(minimumSize: const Size(48, 48)),
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Volver'),
          ),
          TextButton(
            style: TextButton.styleFrom(
              minimumSize: const Size(48, 48),
              foregroundColor: context.colors.statusBlocked,
            ),
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Rechazar plan'),
          ),
        ],
      ),
    ) ??
    false;

/// The note for «Cambiar…», trimmed; null when the user closes the sheet.
Future<String?> askPlanChanges(BuildContext context) => showModalBottomSheet<String>(
  context: context,
  isScrollControlled: true,
  showDragHandle: true,
  builder: (_) => const _ChangesSheet(),
);

class _ChangesSheet extends StatefulWidget {
  const _ChangesSheet();

  @override
  State<_ChangesSheet> createState() => _ChangesSheetState();
}

class _ChangesSheetState extends State<_ChangesSheet> {
  final TextEditingController _note = TextEditingController();
  String? _problem;

  @override
  void dispose() {
    _note.dispose();
    super.dispose();
  }

  void _send() {
    final problem = planNoteProblem(_note.text);
    if (problem != null) {
      setState(() => _problem = problem);
      return;
    }
    Navigator.pop(context, _note.text.trim());
  }

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final problem = _problem;
    return Padding(
      padding: EdgeInsets.fromLTRB(
        Spacing.lg,
        0,
        Spacing.lg,
        Spacing.lg + MediaQuery.viewInsetsOf(context).bottom,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text('¿Qué cambiarías?', style: context.text.title),
          const SizedBox(height: Spacing.md),
          TextField(
            controller: _note,
            autofocus: true,
            minLines: 3,
            maxLines: 5,
            maxLength: planNoteMax,
            keyboardType: TextInputType.multiline,
            decoration: const InputDecoration(
              labelText: 'Qué cambiarías del plan',
            ),
            onChanged: (_) {
              if (_problem != null) setState(() => _problem = null);
            },
          ),
          if (problem != null) ...[
            const SizedBox(height: Spacing.xs),
            Text(
              problem,
              style: context.text.small.copyWith(color: c.accentText),
            ),
          ],
          const SizedBox(height: Spacing.md),
          Row(
            children: [
              Expanded(
                child: FilledButton(
                  style: FilledButton.styleFrom(minimumSize: const Size(0, 48)),
                  onPressed: _send,
                  child: const Text('Enviar cambios'),
                ),
              ),
              const SizedBox(width: Spacing.sm),
              Expanded(
                child: OutlinedButton(
                  style: OutlinedButton.styleFrom(minimumSize: const Size(0, 48)),
                  onPressed: () => Navigator.pop(context),
                  child: const Text('Cancelar'),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
