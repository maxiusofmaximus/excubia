import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../../core/state/chat_document_opener.dart';
import '../../src/rust/api/chat/attach_model.dart';
import '../theme.dart';
import 'chat_document_actions.dart';

const _unavailableText = 'Este archivo no tiene vista previa.';
const _loadFailedText = 'No se pudo cargar la página.';

/// A document one page at a time, zoomable. «Anterior» and «Siguiente» turn
/// the pages; the host says where the document ends.
class DocumentPreviewScreen extends StatefulWidget {
  const DocumentPreviewScreen({
    super.key,
    required this.file,
    required this.documents,
  });

  final ChatAttachment file;
  final ChatDocumentOpener documents;

  @override
  State<DocumentPreviewScreen> createState() => _DocumentPreviewScreenState();
}

class _DocumentPreviewScreenState extends State<DocumentPreviewScreen> {
  int _page = 1;
  bool _atEnd = false;
  bool _opening = false;
  late Future<Uint8List> _picture = _pageOf(1);

  Future<Uint8List> _pageOf(int page) =>
      widget.documents.previews.page(widget.file.id, page);

  void _show(int page) => setState(() {
    _page = page;
    _atEnd = false;
    _picture = _pageOf(page);
  });

  /// Turns to the next page, unless the host says the document ends before it.
  Future<void> _next() async {
    final next = _page + 1;
    try {
      await _pageOf(next);
    } on PreviewRefusal catch (refusal) {
      if (refusal.failure == PreviewFailure.outOfRange) {
        if (mounted) setState(() => _atEnd = true);
        return;
      }
    } catch (_) {
      // Any other failure shows on the page itself, with «Reintentar».
    }
    if (mounted) _show(next);
  }

  void _setOpening(bool busy) {
    if (mounted) setState(() => _opening = busy);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(
          widget.file.name,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        ),
        actions: [
          IconButton(
            tooltip: 'Abrir con otra app',
            icon: const Icon(Icons.open_in_new),
            onPressed: _opening
                ? null
                : () => openDocumentFile(
                    context,
                    widget.documents,
                    widget.file,
                    onBusy: _setOpening,
                  ),
          ),
          IconButton(
            tooltip: 'Guardar',
            icon: const Icon(Icons.save_alt),
            onPressed: () =>
                saveDocumentFile(context, widget.documents, widget.file),
          ),
        ],
      ),
      body: Column(
        children: [
          Expanded(
            child: FutureBuilder<Uint8List>(
              future: _picture,
              builder: (context, snapshot) => _body(snapshot),
            ),
          ),
          SafeArea(
            top: false,
            child: _Pager(
              page: _page,
              canBack: _page > 1,
              canNext: !_atEnd,
              onBack: () => _show(_page - 1),
              onNext: _next,
            ),
          ),
        ],
      ),
    );
  }

  Widget _body(AsyncSnapshot<Uint8List> snapshot) {
    final bytes = snapshot.data;
    if (bytes != null) {
      return Semantics(
        label: 'Página $_page de ${widget.file.name}',
        image: true,
        child: InteractiveViewer(
          maxScale: 5,
          child: Center(child: Image.memory(bytes, gaplessPlayback: true)),
        ),
      );
    }
    if (snapshot.hasError) return _failed(snapshot.error);
    return const Center(child: CircularProgressIndicator());
  }

  Widget _failed(Object? error) {
    final refusal = error is PreviewRefusal ? error : null;
    if (refusal != null &&
        (refusal.failure == PreviewFailure.toolMissing ||
            refusal.failure == PreviewFailure.unavailable)) {
      return _NoPreview(
        message: refusal.message ?? _unavailableText,
        onOpen: () => openDocumentFile(
          context,
          widget.documents,
          widget.file,
          onBusy: _setOpening,
        ),
        onSave: () => saveDocumentFile(context, widget.documents, widget.file),
      );
    }
    return _Retry(onRetry: () => _show(_page));
  }
}

/// The page-turning bar. «Siguiente» is off once the document ends.
class _Pager extends StatelessWidget {
  const _Pager({
    required this.page,
    required this.canBack,
    required this.canNext,
    required this.onBack,
    required this.onNext,
  });

  final int page;
  final bool canBack;
  final bool canNext;
  final VoidCallback onBack;
  final VoidCallback onNext;

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      children: [
        TextButton(
          onPressed: canBack ? onBack : null,
          child: const Text('Anterior'),
        ),
        Text('Página $page', style: context.text.label),
        TextButton(
          onPressed: canNext ? onNext : null,
          child: const Text('Siguiente'),
        ),
      ],
    );
  }
}

/// A document with no picture here: the host's words and the file's own actions.
class _NoPreview extends StatelessWidget {
  const _NoPreview({
    required this.message,
    required this.onOpen,
    required this.onSave,
  });

  final String message;
  final VoidCallback onOpen;
  final VoidCallback onSave;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(Spacing.lg),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              message,
              textAlign: TextAlign.center,
              style: context.text.body,
            ),
            const SizedBox(height: Spacing.md),
            Wrap(
              spacing: Spacing.sm,
              alignment: WrapAlignment.center,
              children: [
                TextButton(
                  onPressed: onOpen,
                  child: const Text('Abrir con otra app'),
                ),
                TextButton(onPressed: onSave, child: const Text('Guardar')),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// A page that did not load: the reason and a retry.
class _Retry extends StatelessWidget {
  const _Retry({required this.onRetry});

  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(_loadFailedText, style: context.text.body),
          TextButton(onPressed: onRetry, child: const Text('Reintentar')),
        ],
      ),
    );
  }
}
