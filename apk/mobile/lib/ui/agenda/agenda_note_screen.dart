import 'package:flutter/material.dart';

import '../../core/api/agenda_api.dart';
import '../theme.dart';
import 'agenda_format.dart';
import 'agenda_widgets.dart';

/// One note: a new one when [id] is null, otherwise the editor of that note.
/// Saving needs `RESPOND`; deleting needs `APPROVE`, and the button says why when
/// the device lacks it.
class AgendaNoteScreen extends StatefulWidget {
  const AgendaNoteScreen({super.key, required this.api, this.id});

  final AgendaApi api;
  final String? id;

  @override
  State<AgendaNoteScreen> createState() => _AgendaNoteScreenState();
}

class _AgendaNoteScreenState extends State<AgendaNoteScreen> {
  final TextEditingController _subject = TextEditingController();
  final TextEditingController _body = TextEditingController();
  bool _loading = false;
  bool _busy = false;
  DateTime? _updatedAt;
  String? _failure;

  @override
  void initState() {
    super.initState();
    if (widget.id != null) _load(widget.id!);
  }

  @override
  void dispose() {
    _subject.dispose();
    _body.dispose();
    super.dispose();
  }

  Future<void> _load(String id) async {
    setState(() => _loading = true);
    try {
      final note = await widget.api.note(id);
      if (!mounted) return;
      _subject.text = note.subject ?? '';
      _body.text = note.body;
      _updatedAt = note.updatedAt;
    } catch (error) {
      if (mounted) _failure = agendaFailureText(error);
    }
    if (mounted) setState(() => _loading = false);
  }

  Future<void> _save() async {
    final id = widget.id;
    final subject = _subject.text.trim();
    await _run(() async {
      if (id == null) {
        await widget.api.createNote(
          subject: subject.isEmpty ? null : subject,
          body: _body.text,
        );
      } else {
        await widget.api.updateNote(id: id, subject: subject, body: _body.text);
      }
    });
  }

  Future<void> _delete() async {
    final sure = await confirmDelete(
      context,
      title: '¿Borrar esta nota?',
      body: 'Se borra ya y no se puede recuperar.',
    );
    if (!sure || !mounted) return;
    final id = widget.id!;
    await _run(() => widget.api.deleteNote(id));
  }

  /// Runs one write. Success closes the editor; a failure stays on screen.
  Future<void> _run(Future<void> Function() action) async {
    setState(() {
      _busy = true;
      _failure = null;
    });
    try {
      await action();
      if (mounted) Navigator.of(context).pop();
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _failure = agendaFailureText(error);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final isNew = widget.id == null;
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(title: Text(isNew ? 'Nueva nota' : 'Nota')),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : ListView(
              padding: const EdgeInsets.all(Spacing.lg),
              children: [
                TextField(
                  controller: _subject,
                  enabled: !_busy,
                  maxLength: 120,
                  style: context.text.body,
                  decoration: const InputDecoration(labelText: 'Asunto'),
                ),
                const SizedBox(height: Spacing.md),
                TextField(
                  controller: _body,
                  enabled: !_busy,
                  minLines: 6,
                  maxLines: null,
                  style: context.text.body,
                  decoration: const InputDecoration(labelText: 'Texto'),
                ),
                if (_updatedAt != null) ...[
                  const SizedBox(height: Spacing.sm),
                  Text(
                    'Actualizada ${relativeDate(_updatedAt!, DateTime.now())}',
                    style: context.text.small,
                  ),
                ],
                if (_failure != null) ...[
                  const SizedBox(height: Spacing.md),
                  Text(_failure!, style: context.text.bodySecondary),
                ],
                const SizedBox(height: Spacing.lg),
                ValueListenableBuilder<TextEditingValue>(
                  valueListenable: _body,
                  builder: (context, value, _) => FilledButton(
                    onPressed: _busy || value.text.trim().isEmpty
                        ? null
                        : _save,
                    child: const Text('Guardar'),
                  ),
                ),
                if (!isNew) ...[
                  const SizedBox(height: Spacing.md),
                  ApproveGrant(
                    api: widget.api,
                    builder: (context, canApprove) => Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        OutlinedButton(
                          onPressed: _busy || canApprove != true
                              ? null
                              : _delete,
                          child: const Text('Borrar nota'),
                        ),
                        if (canApprove == false) ...[
                          const SizedBox(height: Spacing.sm),
                          Text(approveMissingText, style: context.text.small),
                        ],
                      ],
                    ),
                  ),
                ],
              ],
            ),
    );
  }
}
