import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../theme.dart';

/// The collapsed manual path: fingerprint, device id, the host command and the
/// gateway URL. Kept out of the pairing page so that page stays about the
/// single primary flow.
class AdvancedPairingSection extends StatelessWidget {
  const AdvancedPairingSection({
    super.key,
    required this.fingerprint,
    required this.publicKeyCommand,
    required this.coreVersion,
    required this.deviceIdController,
    required this.urlController,
    required this.busy,
    required this.onDeviceIdChanged,
    required this.onSaveAndContinue,
  });

  final String fingerprint;
  final String publicKeyCommand;
  final String coreVersion;
  final TextEditingController deviceIdController;
  final TextEditingController urlController;
  final bool busy;
  final VoidCallback onDeviceIdChanged;
  final VoidCallback onSaveAndContinue;

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
            onChanged: (_) => onDeviceIdChanged(),
            decoration: const InputDecoration(
              labelText: 'Identificador del dispositivo',
            ),
          ),
          const SizedBox(height: Spacing.md),
          Text('Comando para el equipo', style: context.text.label),
          const SizedBox(height: Spacing.xs),
          _CommandBlock(command: publicKeyCommand),
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
              labelText: 'URL del gateway',
              hintText: 'pc.tailnet.ts.net:8788',
            ),
          ),
          const SizedBox(height: Spacing.md),
          OutlinedButton(
            onPressed: busy ? null : onSaveAndContinue,
            child: const Text('Guardar y continuar'),
          ),
        ],
      ),
    );
  }
}

class _CommandBlock extends StatelessWidget {
  const _CommandBlock({required this.command});

  final String command;

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: context.colors.raised,
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: context.colors.outlineSoft),
      ),
      padding: const EdgeInsets.symmetric(
        horizontal: Spacing.md,
        vertical: Spacing.sm,
      ),
      child: Row(
        children: [
          Expanded(
            child: SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: Text(
                command.isEmpty ? '…' : command,
                style: context.text.mono,
              ),
            ),
          ),
          IconButton(
            tooltip: 'Copiar comando',
            icon: const Icon(Icons.copy, size: 20),
            color: context.colors.textSecondary,
            onPressed: command.isEmpty
                ? null
                : () async {
                    await Clipboard.setData(ClipboardData(text: command));
                    if (context.mounted) {
                      ScaffoldMessenger.of(context).showSnackBar(
                        const SnackBar(content: Text('Comando copiado')),
                      );
                    }
                  },
          ),
        ],
      ),
    );
  }
}
