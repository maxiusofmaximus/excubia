import 'package:flutter/material.dart';

import '../../core/api/mail_api.dart';
import '../../core/state/approvals_controller.dart';
import '../../core/state/mail_draft_controller.dart';
import '../../src/rust/api/approvals/model.dart';
import '../chat/approval_format.dart';
import '../theme.dart';

/// The bar pinned under the draft: the send state in one line and the buttons
/// that move it. It stays in view while the draft scrolls.
class MailSendFooter extends StatelessWidget {
  const MailSendFooter({
    super.key,
    required this.controller,
    required this.approvals,
    required this.agentName,
    required this.onEdit,
    required this.onClose,
    this.onRequestChanges,
    this.onOpenSettings,
  });

  final MailDraftController controller;
  final ApprovalsController? approvals;

  /// Who asks for the send, as the chat names them.
  final String agentName;
  final VoidCallback onEdit;
  final VoidCallback onClose;

  /// Null when the sheet is not opened from a chat, so Alfred cannot be asked.
  final VoidCallback? onRequestChanges;

  /// Opens Ajustes › Correo, to reconnect an account. Null hides the button.
  final VoidCallback? onOpenSettings;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return SafeArea(
      top: false,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: colors.raised,
          border: Border(top: BorderSide(color: colors.outlineSoft)),
        ),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(
            Spacing.lg,
            Spacing.sm,
            Spacing.lg,
            Spacing.sm,
          ),
          child: ListenableBuilder(
            listenable: Listenable.merge([controller, ?approvals]),
            builder: (context, _) => _content(context),
          ),
        ),
      ),
    );
  }

  Widget _content(BuildContext context) {
    final draft = controller.draft;
    if (draft == null) return const SizedBox.shrink();
    final approvalId = controller.approvalId;
    final approval = approvalId == null ? null : approvals?.viewOf(approvalId);
    final note = _approvalNote(draft, approval);
    final notice = controller.notice;
    final failure = controller.failure;
    final status = Text(_status(draft), style: context.text.body);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // A failed send's reason is announced once, when it lands.
        if (failure == null)
          status
        else
          Semantics(liveRegion: true, child: status),
        if (note != null) ...[
          const SizedBox(height: Spacing.xs),
          Text(note, style: context.text.bodySecondary),
        ],
        if (notice != null && controller.phase != MailSendPhase.finished) ...[
          const SizedBox(height: Spacing.xs),
          Text(notice, style: context.text.bodySecondary),
        ],
        if (failure?.safe ?? false) ...[
          const SizedBox(height: Spacing.xs),
          Text(
            'Pide la aprobación otra vez',
            style: context.text.bodySecondary,
          ),
        ],
        const SizedBox(height: Spacing.sm),
        Wrap(
          spacing: Spacing.sm,
          runSpacing: Spacing.sm,
          children: _buttons(draft, approval),
        ),
      ],
    );
  }

  String _status(MailDraft draft) => switch (controller.phase) {
    MailSendPhase.idle =>
      'Revísalo con calma. Nada sale hasta que apruebes el envío.',
    MailSendPhase.requesting => 'Preparando la aprobación…',
    MailSendPhase.awaitingApproval => _awaitingLine(draft),
    MailSendPhase.scheduled =>
      'Se envía en ${_secondsLeft()} s, salvo que lo canceles.',
    MailSendPhase.checking => 'Comprobando el resultado del envío…',
    MailSendPhase.finished => controller.notice ?? 'Envío terminado.',
  };

  String _awaitingLine(MailDraft draft) {
    final ends = controller.approvalEndsAt;
    final left = ends?.difference(DateTime.now()).inSeconds ?? 0;
    final to = draft.to.join(', ');
    if (left <= 0) return '$agentName quiere enviar este correo a $to.';
    return '$agentName quiere enviar este correo a $to · '
        'caduca en ${formatCountdown(left)}';
  }

  /// What blocks the approval from this sheet, or null when nothing does. The
  /// approval must show the same facts as the draft, or it is not approved here.
  String? _approvalNote(MailDraft draft, ChatApproval? approval) {
    if (controller.phase != MailSendPhase.awaitingApproval) return null;
    final approvalsNow = approvals;
    if (approvalsNow == null) return 'Aprueba el envío desde el chat.';
    if (approval == null) return null;
    final details = approval.mailSend;
    if (details == null) {
      return 'No se pueden comprobar los datos de este envío. '
          'Apruébalo en el chat.';
    }
    if (!sameMailSend(draft, details, controller.account)) {
      return 'Lo que se aprueba no coincide con este borrador. '
          'No se puede aprobar desde aquí.';
    }
    final id = controller.approvalId;
    if (id != null && approvalsNow.failureOf(id) != null) {
      return 'No se pudo responder. Vuelve a intentarlo.';
    }
    return null;
  }

  List<Widget> _buttons(MailDraft draft, ChatApproval? approval) {
    final phase = controller.phase;
    final approvalsNow = approvals;
    final id = controller.approvalId;
    final busy = id != null && (approvalsNow?.isBusy(id) ?? false);
    final details = approval?.mailSend;
    final open = approval?.status == 'pending' && !busy;
    final canApprove =
        open &&
        details != null &&
        sameMailSend(draft, details, controller.account);
    return [
      if (controller.canRequestSend)
        FilledButton(onPressed: _requestSend, child: const Text('Enviar')),
      if (phase == MailSendPhase.awaitingApproval &&
          approvalsNow != null &&
          id != null) ...[
        FilledButton(
          onPressed: canApprove
              ? () => approvalsNow.decide(id, approve: true)
              : null,
          child: const Text('Aprobar y enviar'),
        ),
        OutlinedButton(
          onPressed: open
              ? () => approvalsNow.decide(id, approve: false)
              : null,
          child: const Text('Denegar'),
        ),
      ],
      if (controller.canCancel)
        FilledButton.tonal(
          onPressed: controller.cancel,
          child: const Text('Cancelar envío'),
        ),
      if (controller.canEdit)
        OutlinedButton(
          onPressed: onEdit,
          child: Text(
            controller.failure == null ? 'Editar' : 'Editar borrador',
          ),
        ),
      if (controller.failure?.code == MailFailureCode.authExpired &&
          onOpenSettings != null)
        OutlinedButton(
          onPressed: onOpenSettings,
          child: const Text('Ir a Ajustes › Correo'),
        ),
      if (phase == MailSendPhase.idle) ...[
        if (onRequestChanges != null)
          TextButton(
            onPressed: onRequestChanges,
            child: const Text('Pedir cambios a Alfred'),
          ),
        TextButton(onPressed: onClose, child: const Text('No enviar')),
      ],
      if (phase == MailSendPhase.finished)
        OutlinedButton(onPressed: onClose, child: const Text('Cerrar')),
    ];
  }

  /// The approval is listed right after the request, so the sheet can show it.
  Future<void> _requestSend() async {
    await controller.requestSend();
    await approvals?.refresh();
  }

  int _secondsLeft() {
    final ends = controller.windowEndsAt;
    if (ends == null) return 0;
    final left = ends.difference(DateTime.now()).inSeconds;
    return left < 0 ? 0 : left;
  }
}
