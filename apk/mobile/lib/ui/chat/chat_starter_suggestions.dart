import 'package:flutter/material.dart';

import '../../core/state/chat_controller.dart';
import '../../core/state/hires_controller.dart';
import '../theme.dart';
import '../widgets/comic/comic_slab.dart';
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
    // The slab leans and keeps its shadow; the text inside stays straight. 48 dp in all.
    return Semantics(
      button: true,
      label: '$title, $detail',
      excludeSemantics: true,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.only(bottom: Spacing.sm),
          child: ComicSlab(
            color: c.deep,
            stroke: c.outlineStrong,
            cut: true,
            child: ConstrainedBox(
              constraints: const BoxConstraints(
                minHeight: 48 - comicShadow - 2 * Spacing.sm,
              ),
              child: Row(
                children: [
                  Icon(icon, size: 20, color: c.onDeep),
                  const SizedBox(width: Spacing.sm),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        Text(title, style: context.text.body),
                        Text(detail, style: context.text.small),
                      ],
                    ),
                  ),
                  Icon(Icons.arrow_forward, size: 18, color: c.onDeep),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
