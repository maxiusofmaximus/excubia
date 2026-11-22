import 'package:flutter/material.dart';

/// Asks once before a task runs now. The next scheduled run stays where it was.
Future<bool> confirmRunNow(BuildContext context, String title) async =>
    await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('Lanzar ahora'),
        content: Text(
          'Se lanzará «$title» ahora. La próxima ejecución sigue en su hora.',
        ),
        actions: [
          TextButton(
            style: TextButton.styleFrom(minimumSize: const Size(48, 48)),
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('Cancelar'),
          ),
          FilledButton(
            style: FilledButton.styleFrom(minimumSize: const Size(48, 48)),
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text('Lanzar'),
          ),
        ],
      ),
    ) ??
    false;
