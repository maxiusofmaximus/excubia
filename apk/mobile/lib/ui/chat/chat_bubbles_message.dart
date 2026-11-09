part of 'chat_bubbles.dart';

/// The step line under the operator's own message, or null when it shows none.
/// Only the newest own message shows every step; an older one shows a failure.
({IconData icon, String text})? turnLine({
  required String? turnState,
  required bool latestOwn,
  String? runningAgent,
}) {
  final line = switch (turnState) {
    'queued' => (icon: Icons.check, text: 'Enviado'),
    'running' => (
      icon: Icons.autorenew,
      text: runningAgent == null
          ? 'Atendiendo'
          : '$runningAgent lo está atendiendo',
    ),
    'completed' => (icon: Icons.done_all, text: 'Respondido'),
    'failed' => (icon: Icons.error_outline, text: 'Falló'),
    'interrupted' => (icon: Icons.stop_circle_outlined, text: 'Detenido'),
    _ => null,
  };
  if (line == null) return null;
  return latestOwn || turnState == 'failed' ? line : null;
}

/// Copies Alfred's reply as it was written, markdown marks included.
class _CopyButton extends StatelessWidget {
  const _CopyButton({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    return Align(
      alignment: AlignmentDirectional.centerStart,
      child: TextButton.icon(
        style: TextButton.styleFrom(
          minimumSize: const Size(48, 48),
          foregroundColor: context.colors.textSecondary,
        ),
        onPressed: () async {
          await Clipboard.setData(ClipboardData(text: text));
          if (!context.mounted) return;
          ScaffoldMessenger.maybeOf(context)
              ?.showSnackBar(const SnackBar(content: Text('Copiado')));
        },
        icon: const Icon(Icons.copy_outlined, size: 16),
        label: const Text('Copiar'),
      ),
    );
  }
}

class _MessageBubble extends StatelessWidget {
  const _MessageBubble({
    required this.entry,
    required this.mine,
    required this.author,
    required this.agentName,
    required this.onRetrySend,
    required this.latestOwn,
    required this.runningAgent,
    required this.suggestions,
    required this.onSuggestion,
    required this.onReply,
    required this.onOpenQuote,
    this.turnStep,
    this.attachments,
    this.images,
    this.documents,
  });

  final ChatEntry entry;
  final bool mine;
  final String author;
  final String agentName;
  final ValueChanged<ChatEntry> onRetrySend;
  final bool latestOwn;
  final String? runningAgent;
  final List<String> suggestions;
  final ValueChanged<String>? onSuggestion;
  final ValueChanged<ChatEntry>? onReply;
  final ValueChanged<String>? onOpenQuote;

  /// The step, under the step line, while the turn runs.
  final String? turnStep;

  /// The files this message names; null or empty shows no thumbnails.
  final List<ChatAttachment>? attachments;
  final ChatAttachmentImages? images;
  final ChatDocumentOpener? documents;

  /// Only a message the host has (not one still sending) can be an answer's target.
  bool get _canReply =>
      onReply != null && entry.messageId != null && !entry.local;

  /// The one action of a long press: answer this message.
  Future<void> _offerReply(BuildContext context) async {
    final reply = await showModalBottomSheet<bool>(
      context: context,
      builder: (sheet) => SafeArea(
        child: ListTile(
          leading: const Icon(Icons.reply),
          title: const Text('Responder'),
          onTap: () => Navigator.pop(sheet, true),
        ),
      ),
    );
    if (reply == true) onReply?.call(entry);
  }

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final sending = entry.local && entry.state == 'sending';
    final failed = entry.local && entry.state == 'failed';
    final turn = mine
        ? turnLine(
            turnState: entry.turnState,
            latestOwn: latestOwn,
            runningAgent: runningAgent,
          )
        : null;
    final step = entry.turnState == 'running' ? turnStep : null;
    final quote = entry.quote;
    final files = attachments ?? const <ChatAttachment>[];
    return GestureDetector(
      onLongPress: _canReply ? () => _offerReply(context) : null,
      child: Align(
        alignment: mine ? Alignment.centerRight : Alignment.centerLeft,
        child: Container(
          margin: EdgeInsets.fromLTRB(
            mine ? 40 : Spacing.lg,
            Spacing.xs,
            mine ? Spacing.lg : 40,
            Spacing.xs,
          ),
          padding: const EdgeInsets.fromLTRB(
            Spacing.md,
            Spacing.sm,
            Spacing.md,
            Spacing.sm,
          ),
          constraints: const BoxConstraints(maxWidth: 520),
          decoration: BoxDecoration(
            color: mine ? c.accentSoft : c.surface,
            borderRadius: BorderRadius.circular(16),
            border: Border.all(color: failed ? c.statusBlocked : c.outlineSoft),
          ),
          child: Column(
            crossAxisAlignment: mine
                ? CrossAxisAlignment.end
                : CrossAxisAlignment.start,
            children: [
              Text(
                author,
                style: context.text.label.copyWith(
                  color: mine ? c.textSecondary : c.accentText,
                ),
              ),
              const SizedBox(height: 2),
              if (quote != null) ...[
                ReplyQuote(
                  quote: quote,
                  author: replyAuthor(quote.authorKind, agentName),
                  onOpen: onOpenQuote == null
                      ? null
                      : () => onOpenQuote!(quote.id),
                ),
                const SizedBox(height: Spacing.xs),
              ],
              if (images case final cache? when files.isNotEmpty && documents != null) ...[
                ChatAttachmentStrip(
                  attachments: files,
                  images: cache,
                  documents: documents!,
                ),
                const SizedBox(height: Spacing.xs),
              ],
              // Alfred's replies carry markdown; the operator's own text is plain.
              if (mine)
                Text(
                  entry.body.isEmpty ? entry.preview : entry.body,
                  style: context.text.body,
                )
              else
                MarkdownText(
                  entry.body.isEmpty ? entry.preview : entry.body,
                  style: context.text.body,
                ),
              if (!mine && entry.body.isNotEmpty) _CopyButton(text: entry.body),
              if (suggestions.isNotEmpty)
                SuggestionChips(texts: suggestions, onTap: onSuggestion),
              if (sending || failed) ...[
                const SizedBox(height: Spacing.xs),
                if (sending)
                  Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      SizedBox(
                        width: 12,
                        height: 12,
                        child: CircularProgressIndicator(
                          strokeWidth: 2,
                          color: c.textSecondary,
                        ),
                      ),
                      const SizedBox(width: Spacing.xs),
                      Text('Enviando…', style: context.text.small),
                    ],
                  )
                else
                  Wrap(
                    crossAxisAlignment: WrapCrossAlignment.center,
                    spacing: Spacing.xs,
                    children: [
                      Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Icon(
                            Icons.error_outline,
                            size: 16,
                            color: c.statusBlocked,
                          ),
                          const SizedBox(width: Spacing.xs),
                          Text('Falló el envío', style: context.text.small),
                        ],
                      ),
                      TextButton(
                        onPressed: () => onRetrySend(entry),
                        style: TextButton.styleFrom(
                          minimumSize: const Size(48, 48),
                          foregroundColor: c.accentText,
                        ),
                        child: const Text('Reintentar'),
                      ),
                    ],
                  ),
              ],
              if (turn case final line?) ...[
                const SizedBox(height: Spacing.xs),
                Semantics(
                  label: line.text,
                  excludeSemantics: true,
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(
                        line.icon,
                        size: 16,
                        color: entry.turnState == 'failed'
                            ? c.statusBlocked
                            : c.textSecondary,
                      ),
                      const SizedBox(width: Spacing.xs),
                      Text(line.text, style: context.text.small),
                    ],
                  ),
                ),
                if (step case final text?)
                  Padding(
                    padding: const EdgeInsets.only(top: 2, left: 22),
                    child: Text(
                      text,
                      style: context.text.small,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
