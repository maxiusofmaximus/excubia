import 'dart:convert';

import 'package:flutter/material.dart';

import '../../core/api/chat_documents.dart';
import '../../core/state/chat_document_opener.dart';
import '../../src/rust/api/chat/attach_model.dart';
import '../theme.dart';
import 'chat_document_actions.dart';

const _loadFailedText = 'No se pudo cargar el archivo.';
const _emptyText = 'El archivo está vacío.';

/// A text document (txt, md, json, csv) read as plain text, top to bottom. The
/// lines are built as they scroll, so a long file stays light.
class TextDocumentScreen extends StatefulWidget {
  const TextDocumentScreen({
    super.key,
    required this.file,
    required this.documents,
  });

  final ChatAttachment file;
  final ChatDocumentOpener documents;

  @override
  State<TextDocumentScreen> createState() => _TextDocumentScreenState();
}

class _TextDocumentScreenState extends State<TextDocumentScreen> {
  bool _opening = false;
  late Future<List<String>> _lines = _load();

  /// The file's bytes as text; bytes that are not UTF-8 show as �, not as an error.
  /// shortcut: the whole file is downloaded and decoded here, on the UI isolate, so a very large file can stutter; decode off the UI isolate or cap the size when files grow.
  Future<List<String>> _load() async {
    final bytes = await widget.documents.download(widget.file);
    return const LineSplitter().convert(
      utf8.decode(bytes, allowMalformed: true),
    );
  }

  void _retry() => setState(() {
    _lines = _load();
  });

  void _setOpening(bool busy) {
    if (mounted) setState(() => _opening = busy);
  }

  @override
  Widget build(BuildContext context) {
    final ext = extensionOf(widget.file.name);
    final style = ext == 'json' || ext == 'csv'
        ? context.text.mono
        : context.text.body;
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
      body: FutureBuilder<List<String>>(
        future: _lines,
        builder: (context, snapshot) {
          final lines = snapshot.data;
          if (lines != null) {
            if (lines.isEmpty) {
              return Center(child: Text(_emptyText, style: context.text.body));
            }
            return ListView.builder(
              padding: const EdgeInsets.all(Spacing.lg),
              itemCount: lines.length,
              itemBuilder: (_, index) => Text(lines[index], style: style),
            );
          }
          if (snapshot.hasError) {
            return _Failed(
              onRetry: _retry,
              onOpen: () => openDocumentFile(
                context,
                widget.documents,
                widget.file,
                onBusy: _setOpening,
              ),
            );
          }
          return const Center(child: CircularProgressIndicator());
        },
      ),
    );
  }
}

/// A file that did not load: the reason, a retry, and the system app as the way out.
class _Failed extends StatelessWidget {
  const _Failed({required this.onRetry, required this.onOpen});

  final VoidCallback onRetry;
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(Spacing.lg),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(_loadFailedText, style: context.text.body),
            const SizedBox(height: Spacing.md),
            Wrap(
              spacing: Spacing.sm,
              alignment: WrapAlignment.center,
              children: [
                TextButton(onPressed: onRetry, child: const Text('Reintentar')),
                TextButton(
                  onPressed: onOpen,
                  child: const Text('Abrir con otra app'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
