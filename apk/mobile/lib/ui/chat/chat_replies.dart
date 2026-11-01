import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';

import '../theme.dart';

/// Who wrote a message as the reply line names it.
String replyAuthor(String? authorKind, String agentName) =>
    authorKind == 'user' ? 'Tú' : agentName;

/// Alfred's one-tap replies, under his message. Each chip sends its text as a
/// normal message; the text is plain, never markdown. A null [onTap] shows
/// them disabled, while a send is under way.
class SuggestionChips extends StatelessWidget {
  const SuggestionChips({super.key, required this.texts, required this.onTap});

  final List<String> texts;
  final ValueChanged<String>? onTap;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(top: Spacing.xs),
      child: Wrap(
        spacing: Spacing.xs,
        runSpacing: Spacing.xs,
        children: [
          for (final text in texts)
            Semantics(
              button: true,
              label: text,
              child: OutlinedButton(
                onPressed: onTap == null ? null : () => onTap!(text),
                style: OutlinedButton.styleFrom(
                  minimumSize: const Size(48, 48),
                ),
                child: Text(text),
              ),
            ),
        ],
      ),
    );
  }
}

/// The message a reply answers, above its body. A button: a tap asks the screen
/// to scroll to the original when it is loaded.
class ReplyQuote extends StatelessWidget {
  const ReplyQuote({
    super.key,
    required this.quote,
    required this.author,
    required this.onOpen,
  });

  final ChatQuote quote;
  final String author;
  final VoidCallback? onOpen;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Semantics(
      container: true,
      button: true,
      label: 'Responde a $author: ${quote.preview}',
      excludeSemantics: true,
      child: InkWell(
        onTap: onOpen,
        child: Container(
          constraints: const BoxConstraints(minHeight: 48),
          padding: const EdgeInsets.only(left: Spacing.sm, top: 2, bottom: 2),
          decoration: BoxDecoration(
            border: Border(left: BorderSide(color: c.accentText, width: 3)),
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Text(
                author,
                style: context.text.label.copyWith(color: c.textSecondary),
              ),
              Text(
                quote.preview,
                style: context.text.small,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// «Respondiendo a …» above the composer, with the way out of the reply.
class ReplyBar extends StatelessWidget {
  const ReplyBar({
    super.key,
    required this.target,
    required this.agentName,
    required this.onCancel,
  });

  final ChatEntry target;
  final String agentName;
  final VoidCallback onCancel;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final author = replyAuthor(target.authorKind, agentName);
    return Container(
      padding: const EdgeInsets.only(left: Spacing.md, right: Spacing.xs),
      decoration: BoxDecoration(
        color: c.surface,
        border: Border(top: BorderSide(color: c.outlineSoft)),
      ),
      child: Row(
        children: [
          Expanded(
            child: Text(
              'Respondiendo a $author: ${target.preview}',
              style: context.text.small,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
          ),
          Semantics(
            button: true,
            label: 'Cancelar respuesta',
            child: IconButton(
              tooltip: 'Cancelar respuesta',
              style: IconButton.styleFrom(minimumSize: const Size(48, 48)),
              onPressed: onCancel,
              icon: const Icon(Icons.close),
            ),
          ),
        ],
      ),
    );
  }
}
