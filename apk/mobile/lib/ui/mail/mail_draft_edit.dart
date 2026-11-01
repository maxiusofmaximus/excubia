import 'package:flutter/material.dart';

import '../../core/state/mail_draft_controller.dart';
import '../theme.dart';

/// Edits the content of a draft. «Guardar» sends all of it to the host, which
/// checks it again; a saved change means the send must be approved again.
class MailDraftEdit extends StatefulWidget {
  const MailDraftEdit({
    super.key,
    required this.controller,
    required this.onDone,
  });

  final MailDraftController controller;

  /// Called after a save or a cancel: the sheet shows the draft again.
  final VoidCallback onDone;

  @override
  State<MailDraftEdit> createState() => _MailDraftEditState();
}

class _MailDraftEditState extends State<MailDraftEdit> {
  late final TextEditingController _to;
  late final TextEditingController _cc;
  late final TextEditingController _subject;
  late final TextEditingController _body;

  @override
  void initState() {
    super.initState();
    final draft = widget.controller.draft!;
    _to = TextEditingController(text: draft.to.join(', '));
    _cc = TextEditingController(text: draft.cc.join(', '));
    _subject = TextEditingController(text: draft.subject);
    _body = TextEditingController(text: draft.body);
  }

  @override
  void dispose() {
    _to.dispose();
    _cc.dispose();
    _subject.dispose();
    _body.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final saved = await widget.controller.update(
      to: addressesOf(_to.text),
      cc: addressesOf(_cc.text),
      subject: _subject.text,
      body: _body.text,
    );
    if (saved && mounted) widget.onDone();
  }

  @override
  Widget build(BuildContext context) {
    final text = context.text;
    final controller = widget.controller;
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Si cambias el borrador hay que aprobar el envío de nuevo.',
            style: text.bodySecondary,
          ),
          const SizedBox(height: Spacing.md),
          TextField(
            controller: _to,
            keyboardType: TextInputType.emailAddress,
            decoration: const InputDecoration(labelText: 'Para'),
          ),
          const SizedBox(height: Spacing.md),
          TextField(
            controller: _cc,
            keyboardType: TextInputType.emailAddress,
            decoration: const InputDecoration(labelText: 'CC'),
          ),
          const SizedBox(height: Spacing.md),
          TextField(
            controller: _subject,
            decoration: const InputDecoration(labelText: 'Asunto'),
          ),
          const SizedBox(height: Spacing.md),
          TextField(
            controller: _body,
            minLines: 6,
            maxLines: null,
            decoration: const InputDecoration(labelText: 'Cuerpo'),
          ),
          if (controller.notice case final notice?) ...[
            const SizedBox(height: Spacing.md),
            Text(notice, style: text.bodySecondary),
          ],
          const SizedBox(height: Spacing.lg),
          Row(
            children: [
              Expanded(
                child: FilledButton(
                  onPressed: controller.busy ? null : _save,
                  child: const Text('Guardar'),
                ),
              ),
              const SizedBox(width: Spacing.sm),
              Expanded(
                child: OutlinedButton(
                  onPressed: controller.busy ? null : widget.onDone,
                  child: const Text('Cancelar'),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// Addresses typed apart by commas, semicolons or spaces. The host has the last
/// word on them; Rust checks the shape before the call.
List<String> addressesOf(String raw) => [
  for (final part in raw.split(RegExp(r'[,;\s]+')))
    if (part.isNotEmpty) part,
];
