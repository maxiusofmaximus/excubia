import 'package:flutter/material.dart';

import '../../core/api/chat_documents.dart';
import '../../core/state/chat_document_opener.dart';
import '../../src/rust/api/chat/attach_model.dart';
import '../mail/mail_facts.dart';
import '../theme.dart';
import 'chat_document_actions.dart';
import 'document_preview_screen.dart';
import 'text_document_screen.dart';

IconData documentIcon(ChatDocKind kind) => switch (kind) {
  ChatDocKind.sheet => Icons.table_chart_outlined,
  ChatDocKind.word => Icons.description_outlined,
  ChatDocKind.slides => Icons.slideshow_outlined,
  ChatDocKind.pdf => Icons.picture_as_pdf_outlined,
  ChatDocKind.text => Icons.notes_outlined,
};

/// A document in a message: its icon, name and size. A tap on an xlsx, docx or
/// pdf opens its pages; a txt, md, json or csv opens as text; any other
/// document goes to the system app.
class ChatDocumentCard extends StatefulWidget {
  const ChatDocumentCard({
    super.key,
    required this.file,
    required this.documents,
  });

  final ChatAttachment file;
  final ChatDocumentOpener documents;

  @override
  State<ChatDocumentCard> createState() => _ChatDocumentCardState();
}

class _ChatDocumentCardState extends State<ChatDocumentCard> {
  bool _opening = false;

  void _tap() {
    final name = widget.file.name;
    if (isPreviewableDocument(name)) {
      _push(
        DocumentPreviewScreen(file: widget.file, documents: widget.documents),
      );
    } else if (isTextDocument(name)) {
      _push(TextDocumentScreen(file: widget.file, documents: widget.documents));
    } else {
      _open();
    }
  }

  void _push(Widget screen) =>
      Navigator.of(context)
          .push(MaterialPageRoute<void>(builder: (_) => screen));

  void _open() {
    if (_opening) return;
    openDocumentFile(
      context,
      widget.documents,
      widget.file,
      onBusy: (busy) {
        if (mounted) setState(() => _opening = busy);
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final kind = documentKindOf(widget.file.name) ?? ChatDocKind.text;
    return Semantics(
      button: true,
      label: '${kind.label} ${widget.file.name}',
      child: Material(
        color: c.raised,
        borderRadius: BorderRadius.circular(12),
        child: InkWell(
          borderRadius: BorderRadius.circular(12),
          onTap: _tap,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minWidth: 180, maxWidth: 260),
            child: Padding(
              padding: const EdgeInsets.all(Spacing.sm),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  _Badge(busy: _opening, icon: documentIcon(kind)),
                  const SizedBox(width: Spacing.sm),
                  Flexible(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(
                          widget.file.name,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: context.text.small,
                        ),
                        Text(
                          sizeLabel(widget.file.size.toInt()),
                          style: context.text.label.copyWith(
                            color: c.textMuted,
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// The icon of a document, or a spinner while it is downloaded.
class _Badge extends StatelessWidget {
  const _Badge({required this.busy, required this.icon});

  final bool busy;
  final IconData icon;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Container(
      width: 40,
      height: 40,
      decoration: BoxDecoration(
        color: c.surface,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Center(
        child: busy
            ? const SizedBox(
                width: 20,
                height: 20,
                child: CircularProgressIndicator(strokeWidth: 2),
              )
            : Icon(icon, color: c.textMuted),
      ),
    );
  }
}
