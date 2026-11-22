import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/state/approvals_controller.dart';
import '../../core/state/mail_draft_controller.dart';
import '../theme.dart';
import '../widgets/surface_card.dart';
import 'mail_draft_edit.dart';
import 'mail_facts.dart';
import 'mail_send_footer.dart';

/// What the operator chose on the draft sheet that the chat must act on.
enum MailDraftChoice { requestChanges }

/// Opens «Revisar borrador» as a sheet that covers almost the whole screen. The
/// send lives in [controller], which outlives the sheet until the send ends.
/// [approvals] answers the approval from the footer, and [agentName] names who
/// asks for the send. [alfredCanRevise] adds «Pedir cambios a Alfred», which the
/// chat answers by naming the draft. [onOpenSettings] adds «Ir a Ajustes › Correo»
/// when a failed send needs the account reconnected.
Future<MailDraftChoice?> showMailDraftSheet(
  BuildContext context, {
  required MailDraftController controller,
  ApprovalsController? approvals,
  bool alfredCanRevise = false,
  String agentName = 'Un agente',
  VoidCallback? onOpenSettings,
}) {
  return showModalBottomSheet<MailDraftChoice>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    backgroundColor: context.colors.bg,
    builder: (_) => SizedBox(
      height: MediaQuery.sizeOf(context).height * 0.94,
      child: MailDraftScreen(
        controller: controller,
        approvals: approvals,
        alfredCanRevise: alfredCanRevise,
        agentName: agentName,
        onOpenSettings: onOpenSettings,
      ),
    ),
  );
}

/// Reads one draft and drives its send. «Enviar» only asks: the send waits for
/// the approval, which the operator gives in the footer of this sheet.
class MailDraftScreen extends StatefulWidget {
  const MailDraftScreen({
    super.key,
    required this.controller,
    this.approvals,
    this.alfredCanRevise = false,
    this.agentName = 'Un agente',
    this.onOpenSettings,
  });

  /// Owned by the caller: closing this screen does not end the send it shows.
  final MailDraftController controller;
  final ApprovalsController? approvals;
  final bool alfredCanRevise;
  final String agentName;
  final VoidCallback? onOpenSettings;

  @override
  State<MailDraftScreen> createState() => _MailDraftScreenState();
}

class _MailDraftScreenState extends State<MailDraftScreen> {
  MailDraftController get _controller => widget.controller;
  bool _editing = false;

  @override
  void initState() {
    super.initState();
    // A draft reopened mid-send already has its draft; only read it when missing.
    if (_controller.draft == null) unawaited(_controller.load());
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(title: const Text('Revisar borrador')),
      body: ListenableBuilder(
        listenable: _controller,
        builder: (context, _) => ListView(
          padding: const EdgeInsets.all(Spacing.lg),
          children: _sections(context),
        ),
      ),
      // Pinned, so «Aprobar y enviar» is in view without scrolling. Editing has
      // its own buttons, so the bar steps aside while the fields are open.
      bottomNavigationBar: _editing
          ? null
          : MailSendFooter(
              controller: _controller,
              approvals: widget.approvals,
              agentName: widget.agentName,
              onEdit: () => setState(() => _editing = true),
              onClose: () => Navigator.of(context).pop(),
              onRequestChanges: widget.alfredCanRevise
                  ? () =>
                        Navigator.of(context)
                            .pop(MailDraftChoice.requestChanges)
                  : null,
              onOpenSettings: widget.onOpenSettings,
            ),
    );
  }

  /// The draft is shown once here; the approval and its buttons live in the footer.
  List<Widget> _sections(BuildContext context) {
    final controller = _controller;
    final draft = controller.draft;
    if (controller.loadFailure case final failure?) {
      return [
        SurfaceCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(failure, style: context.text.body),
              const SizedBox(height: Spacing.sm),
              TextButton(
                onPressed: controller.busy ? null : controller.load,
                child: const Text('Reintentar'),
              ),
            ],
          ),
        ),
      ];
    }
    if (draft == null) {
      return [
        if (controller.busy) const Center(child: CircularProgressIndicator()),
      ];
    }
    if (_editing) {
      return [
        MailDraftEdit(
          controller: controller,
          onDone: () => setState(() => _editing = false),
        ),
      ];
    }
    return [
      SurfaceCard(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            MailFactRow(label: 'CUENTA', value: controller.account),
            MailFactRow(
              label: 'REMITENTE',
              value: senderLabel(draft.from, controller.account),
            ),
            MailFactRow(label: 'PARA', value: draft.to.join(', ')),
            if (draft.cc.isNotEmpty)
              MailFactRow(label: 'CC', value: draft.cc.join(', ')),
            MailFactRow(label: 'ASUNTO', value: draft.subject),
            MailFactRow(label: 'CUERPO', value: draft.body),
            for (final attachment in draft.attachments)
              MailAttachmentRow(
                name: attachment.name,
                size: attachment.size,
                sha256: attachment.sha256,
              ),
          ],
        ),
      ),
    ];
  }
}
