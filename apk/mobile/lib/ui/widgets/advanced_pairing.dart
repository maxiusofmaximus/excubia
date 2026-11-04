import 'package:flutter/material.dart';

import '../theme.dart';

/// The collapsed manual path: fingerprint, device id and the host URL. Kept out
/// of the pairing page so that page stays about the single primary flow.
class AdvancedPairingSection extends StatelessWidget {
  const AdvancedPairingSection({
    super.key,
    required this.fingerprint,
    required this.coreVersion,
    required this.deviceIdController,
    required this.urlController,
  });

  final String fingerprint;
  final String coreVersion;
  final TextEditingController deviceIdController;
  final TextEditingController urlController;

  @override
  Widget build(BuildContext context) {
    final radius = BorderRadius.circular(16);
    final outline = RoundedRectangleBorder(
      borderRadius: radius,
      side: BorderSide(color: context.colors.outlineSoft),
    );
    return Theme(
      data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
      child: ExpansionTile(
        title: Text('Avanzado', style: context.text.title),
        iconColor: context.colors.textSecondary,
        collapsedIconColor: context.colors.textSecondary,
        backgroundColor: context.colors.surface,
        collapsedBackgroundColor: context.colors.surface,
        shape: outline,
        collapsedShape: outline,
        childrenPadding: const EdgeInsets.fromLTRB(
          Spacing.lg,
          0,
          Spacing.lg,
          Spacing.lg,
        ),
        expandedCrossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Huella de la clave', style: context.text.label),
          const SizedBox(height: Spacing.xs),
          Text(fingerprint, style: context.text.mono),
          const SizedBox(height: Spacing.xs),
          Text('Núcleo Rust $coreVersion', style: context.text.small),
          const SizedBox(height: Spacing.md),
          TextField(
            controller: deviceIdController,
            style: context.text.mono,
            decoration: const InputDecoration(
              labelText: 'Identificador del dispositivo',
            ),
          ),
          const SizedBox(height: Spacing.md),
          TextField(
            controller: urlController,
            style: context.text.mono,
            keyboardType: TextInputType.url,
            // The stored URL is selected on tap, so typing replaces it instead of
            // appending to it.
            onTap: () => urlController.selection = TextSelection(
              baseOffset: 0,
              extentOffset: urlController.text.length,
            ),
            decoration: const InputDecoration(
              labelText: 'Dirección del PC',
              hintText: '192.168.1.20:47443',
            ),
          ),
        ],
      ),
    );
  }
}
