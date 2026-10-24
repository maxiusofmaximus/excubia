import 'package:flutter/material.dart';

import '../theme.dart';

class ChatComposer extends StatelessWidget {
  const ChatComposer({
    super.key,
    required this.controller,
    required this.focus,
    required this.enabled,
    required this.sending,
    required this.offline,
    required this.onSend,
  });

  final TextEditingController controller;
  final FocusNode focus;
  final bool enabled;
  final bool sending;
  final bool offline;
  final VoidCallback onSend;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Container(
      padding: const EdgeInsets.fromLTRB(
        Spacing.md,
        Spacing.sm,
        Spacing.md,
        Spacing.sm,
      ),
      decoration: BoxDecoration(
        color: c.surface,
        border: Border(top: BorderSide(color: c.outlineSoft)),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          Expanded(
            child: TextField(
              controller: controller,
              focusNode: focus,
              enabled: enabled,
              minLines: 1,
              maxLines: 4,
              textInputAction: TextInputAction.newline,
              style: context.text.body,
              decoration: InputDecoration(
                hintText: offline
                    ? 'Reconecta para escribir a Alfred'
                    : 'Mensaje a Alfred',
                isDense: true,
                contentPadding: const EdgeInsets.symmetric(
                  horizontal: Spacing.md,
                  vertical: Spacing.md,
                ),
              ),
            ),
          ),
          const SizedBox(width: Spacing.sm),
          Semantics(
            button: true,
            label: 'Enviar mensaje',
            child: IconButton.filled(
              onPressed: enabled ? onSend : null,
              tooltip: 'Enviar',
              style: IconButton.styleFrom(
                minimumSize: const Size(48, 48),
                backgroundColor: c.accent,
                foregroundColor: c.onAccent,
                disabledBackgroundColor: c.raised,
                disabledForegroundColor: c.textMuted,
              ),
              icon: sending
                  ? SizedBox(
                      width: 18,
                      height: 18,
                      child: CircularProgressIndicator(
                        strokeWidth: 2,
                        color: c.onAccent,
                      ),
                    )
                  : const Icon(Icons.arrow_upward),
            ),
          ),
        ],
      ),
    );
  }
}
