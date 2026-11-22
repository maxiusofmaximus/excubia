part of 'chat_bubbles.dart';

/// The step line under the operator's own message, or null when it shows none.
/// Only the newest own message shows every step; an older one shows a failure.
({IconData icon, String text})? turnLine({
  required String? turnState,
  required bool latestOwn,
  String? runningAgent,
  bool replyBelow = false,
}) {
  // The reply sits right under the message: «Respondido» would only repeat it.
  if (turnState == 'completed' && replyBelow) return null;
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

/// What a long press on a message can start.
enum _MessageAction { reply, copy, select }

/// Copies a reply as it was written, markdown marks included, and says so.
Future<void> _copyText(BuildContext context, String text) async {
  await Clipboard.setData(ClipboardData(text: text));
  if (!context.mounted) return;
  ScaffoldMessenger.maybeOf(context)
      ?.showSnackBar(const SnackBar(content: Text('Copiado')));
}

/// Under Alfred's reply: one small icon to copy it, with a 48 dp touch target.
class _ReplyActions extends StatelessWidget {
  const _ReplyActions({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    return Align(
      alignment: AlignmentDirectional.centerStart,
      child: Semantics(
        button: true,
        label: 'Copiar respuesta',
        child: IconButton(
          tooltip: 'Copiar respuesta',
          onPressed: () => _copyText(context, text),
          icon: const Icon(Icons.copy_outlined),
          iconSize: 18,
          color: context.colors.textSecondary,
          constraints: const BoxConstraints(minWidth: 48, minHeight: 48),
        ),
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
    this.characterId,
    this.turnStep,
    this.attachments,
    this.images,
    this.documents,
    this.replyBelow = false,
    this.startsStreak = true,
  });

  final ChatEntry entry;
  final bool mine;
  final String author;
  final String agentName;

  /// The face beside the author's name; null shows the name alone.
  final String? characterId;
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

  /// True when Alfred's answer is the next row, so the step line need not repeat it.
  final bool replyBelow;

  /// True when the row above is not Alfred's: this bubble gets the tail toward the face.
  final bool startsStreak;

  /// Only a message the host has (not one still sending) can be an answer's target.
  bool get _canReply =>
      onReply != null && entry.messageId != null && !entry.local;

  /// A long press answers a message the host has, and opens the actions of an Alfred reply.
  bool get _canOpenActions => _canReply || (!mine && entry.body.isNotEmpty);

  /// Answer, copy or select: one sheet, only the actions that apply to this message.
  Future<void> _openActions(BuildContext context) async {
    final action = await showModalBottomSheet<_MessageAction>(
      context: context,
      builder: (sheet) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (_canReply)
              ListTile(
                leading: const Icon(Icons.reply),
                title: const Text('Responder'),
                onTap: () => Navigator.pop(sheet, _MessageAction.reply),
              ),
            if (!mine) ...[
              ListTile(
                leading: const Icon(Icons.copy_outlined),
                title: const Text('Copiar'),
                onTap: () => Navigator.pop(sheet, _MessageAction.copy),
              ),
              ListTile(
                leading: const Icon(Icons.select_all),
                title: const Text('Seleccionar texto'),
                onTap: () => Navigator.pop(sheet, _MessageAction.select),
              ),
            ],
          ],
        ),
      ),
    );
    if (!context.mounted) return;
    switch (action) {
      case _MessageAction.reply:
        onReply?.call(entry);
      case _MessageAction.copy:
        await _copyText(context, entry.body);
      case _MessageAction.select:
        await _selectText(context);
      case null:
    }
  }

  /// The reply as selectable text: a long press on a word selects it, and the handles widen it.
  Future<void> _selectText(BuildContext context) => showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    builder: (sheet) => SafeArea(
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(Spacing.lg),
        child: SelectableText(entry.body, style: context.text.body),
      ),
    ),
  );

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
            replyBelow: replyBelow,
          )
        : null;
    final step = entry.turnState == 'running' ? turnStep : null;
    final quote = entry.quote;
    final files = attachments ?? const <ChatAttachment>[];
    // Text on the hot slab is onHot; elsewhere it keeps the usual colours.
    final body = context.text.body.copyWith(color: mine ? c.onHot : null);
    final small = context.text.small.copyWith(color: mine ? c.onHot : null);
    final bubble = ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 520),
      child: ComicSlab(
        color: mine ? c.hot : c.deep,
        cut: mine,
        tail: !mine && startsStreak,
        stroke: failed ? c.statusBlocked : null,
        padding: const EdgeInsets.fromLTRB(
          Spacing.md,
          Spacing.sm,
          Spacing.md,
          Spacing.sm,
        ),
        child: Column(
          crossAxisAlignment: mine
              ? CrossAxisAlignment.end
              : CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              author,
              style: context.text.label.copyWith(
                color: mine ? c.onHot : c.accentText,
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
            if (images case final cache?
                when files.isNotEmpty && documents != null) ...[
              ChatAttachmentStrip(
                attachments: files,
                images: cache,
                documents: documents!,
              ),
              const SizedBox(height: Spacing.xs),
            ],
            // Alfred's replies carry markdown; the operator's own text is plain.
            if (mine)
              Text(entry.body.isEmpty ? entry.preview : entry.body, style: body)
            else
              MarkdownText(
                entry.body.isEmpty ? entry.preview : entry.body,
                style: body,
              ),
            if (!mine && entry.body.isNotEmpty) _ReplyActions(text: entry.body),
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
                        color: mine ? c.onHot : c.textSecondary,
                      ),
                    ),
                    const SizedBox(width: Spacing.xs),
                    Text('Enviando…', style: small),
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
                        Text('Falló el envío', style: small),
                      ],
                    ),
                    TextButton(
                      onPressed: () => onRetrySend(entry),
                      style: TextButton.styleFrom(
                        minimumSize: const Size(48, 48),
                        foregroundColor: mine ? c.onHot : c.accentText,
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
                          : (mine ? c.onHot : c.textSecondary),
                    ),
                    const SizedBox(width: Spacing.xs),
                    Text(line.text, style: small),
                  ],
                ),
              ),
              if (step case final text?)
                Padding(
                  padding: const EdgeInsets.only(top: 2, left: 22),
                  child: Text(
                    text,
                    style: small,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
            ],
          ],
        ),
      ),
    );
    // Alfred's replies carry his face at 44 dp, outside the bubble; the tail on the
    // first bubble of a run points at it.
    final row = mine
        ? Padding(
            padding: const EdgeInsets.fromLTRB(
              40,
              Spacing.xs,
              Spacing.lg,
              Spacing.xs,
            ),
            child: Align(
              alignment: AlignmentDirectional.centerEnd,
              child: bubble,
            ),
          )
        : Padding(
            padding: const EdgeInsets.fromLTRB(
              Spacing.lg,
              Spacing.xs,
              40,
              Spacing.xs,
            ),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (characterId case final id?) ...[
                  Padding(
                    padding: const EdgeInsets.only(right: Spacing.sm),
                    child: ExcludeSemantics(
                      child: AgentAvatar(
                        name: author,
                        characterId: id,
                        size: 44,
                        framed: true,
                        animated: false,
                      ),
                    ),
                  ),
                ],
                Flexible(child: bubble),
              ],
            ),
          );
    return GestureDetector(
      onLongPress: _canOpenActions ? () => _openActions(context) : null,
      child: row,
    );
  }
}
