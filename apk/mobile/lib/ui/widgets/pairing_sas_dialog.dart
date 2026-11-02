import 'package:flutter/material.dart';

import '../theme.dart';

/// The string both screens must show after a pinned pairing: the Master compares
/// them before granting anything (RFC pairing-security §5). It cannot be
/// dismissed by tapping outside, so it is never skipped by mistake.
Future<void> showPairingSasDialog(BuildContext context, String sas) {
  final grouped = '${sas.substring(0, 3)} ${sas.substring(3)}';
  return showDialog<void>(
    context: context,
    barrierDismissible: false,
    builder: (context) => AlertDialog(
      backgroundColor: context.colors.raised,
      title: const Text('Comprueba esta cadena'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'En el PC debe aparecer la misma:',
            style: context.text.bodySecondary,
          ),
          const SizedBox(height: Spacing.md),
          Text(grouped, style: context.text.mono.copyWith(fontSize: 32)),
          const SizedBox(height: Spacing.md),
          Text(
            'Si no coincide o no la ves, no concedas permisos y revoca este '
            'dispositivo desde el PC.',
            style: context.text.bodySecondary,
          ),
        ],
      ),
      actions: [
        FilledButton(
          style: FilledButton.styleFrom(minimumSize: const Size(88, 44)),
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Entendido'),
        ),
      ],
    ),
  );
}
