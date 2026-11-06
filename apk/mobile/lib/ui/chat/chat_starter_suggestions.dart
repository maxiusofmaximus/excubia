import 'package:flutter/material.dart';

import '../../core/state/chat_controller.dart';
import '../../core/state/hires_controller.dart';
import '../theme.dart';
import 'chat_suggestions.dart';

/// The starter suggestions of an empty chat or of a new conversation with no message yet.
class StarterSuggestions extends StatelessWidget {
  const StarterSuggestions({
    super.key,
    required this.chat,
    required this.hires,
    required this.onSuggestion,
  });

  final ChatController chat;
  final HiresController hires;
  final ValueChanged<String> onSuggestion;

  static const _icons = [
    Icons.help_outline,
    Icons.work_outline,
    Icons.edit_note,
  ];

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: hires,
      builder: (context, _) {
        // Cambia al crear chats, así no salen siempre las mismas frases.
        final suggestions = chatSuggestions(
          hires: hires.hires,
          rotation: chat.conversations.length,
        );
        return Column(
          children: [
            for (final (index, suggestion) in suggestions.indexed)
              _StarterTile(
                icon: _icons[index],
                title: suggestion.text,
                detail: suggestion.detail,
                onTap: () => onSuggestion(suggestion.text),
              ),
          ],
        );
      },
    );
  }
}

class _StarterTile extends StatelessWidget {
  const _StarterTile({
    required this.icon,
    required this.title,
    required this.detail,
    required this.onTap,
  });

  final IconData icon;
  final String title;
  final String detail;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Semantics(
      button: true,
      label: '$title, $detail',
      child: Material(
        color: c.surface,
        borderRadius: BorderRadius.circular(16),
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(16),
          child: Container(
            constraints: const BoxConstraints(minHeight: 56),
            margin: const EdgeInsets.only(bottom: Spacing.sm),
            padding: const EdgeInsets.all(Spacing.md),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(16),
              border: Border.all(color: c.outlineSoft),
            ),
            child: Row(
              children: [
                Container(
                  width: 30,
                  height: 30,
                  decoration: BoxDecoration(
                    color: c.accentSoft,
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Icon(icon, size: 18, color: c.accentText),
                ),
                const SizedBox(width: Spacing.sm),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(title, style: context.text.body),
                      Text(detail, style: context.text.small),
                    ],
                  ),
                ),
                Icon(Icons.arrow_forward, size: 18, color: c.textMuted),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
