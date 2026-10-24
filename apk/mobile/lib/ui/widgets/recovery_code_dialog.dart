import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../theme.dart';

/// The recovery code is shown once, right after pairing, and never again.
Future<void> showRecoveryCodeDialog(BuildContext context, String code) {
  return showDialog<void>(
    context: context,
    barrierDismissible: false,
    builder: (context) => AlertDialog(
      backgroundColor: context.colors.raised,
      title: const Text('Código de recuperación'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Guárdalo: no se vuelve a mostrar. Sirve para re-emparejar este '
            'teléfono sin aprobación manual.',
            style: context.text.bodySecondary,
          ),
          const SizedBox(height: Spacing.md),
          SelectableText(code, style: context.text.mono),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () async {
            await Clipboard.setData(ClipboardData(text: code));
            if (context.mounted) {
              ScaffoldMessenger.of(
                context,
              ).showSnackBar(const SnackBar(content: Text('Código copiado')));
            }
          },
          child: const Text('Copiar'),
        ),
        FilledButton(
          style: FilledButton.styleFrom(minimumSize: const Size(88, 44)),
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Hecho'),
        ),
      ],
    ),
  );
}
