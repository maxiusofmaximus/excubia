import 'package:flutter/material.dart';

import '../theme.dart';

/// Asks for a pairing link the operator copied from the host. Resolves to the
/// trimmed text, or to null when the operator cancels.
Future<String?> showPasteLinkDialog(BuildContext context) {
  return showDialog<String>(
    context: context,
    builder: (context) => const _PasteLinkDialog(),
  );
}

class _PasteLinkDialog extends StatefulWidget {
  const _PasteLinkDialog();

  @override
  State<_PasteLinkDialog> createState() => _PasteLinkDialogState();
}

class _PasteLinkDialogState extends State<_PasteLinkDialog> {
  final TextEditingController _link = TextEditingController();

  @override
  void dispose() {
    _link.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: context.colors.raised,
      title: const Text('Pegar enlace'),
      content: TextField(
        controller: _link,
        autofocus: true,
        minLines: 1,
        maxLines: 4,
        style: context.text.mono,
        decoration: const InputDecoration(hintText: 'herdr-notify://pair?'),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancelar'),
        ),
        FilledButton(
          style: FilledButton.styleFrom(minimumSize: const Size(88, 44)),
          onPressed: () => Navigator.of(context).pop(_link.text.trim()),
          child: const Text('Usar'),
        ),
      ],
    );
  }
}
