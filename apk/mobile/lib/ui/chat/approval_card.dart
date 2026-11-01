import 'dart:async';

import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/approvals/error.dart';
import 'package:herdr_notify/src/rust/api/approvals/model.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';
import 'package:herdr_notify/src/rust/api/state.dart';

import '../../core/state/approval_move.dart';
import '../../core/state/approvals_controller.dart';
import '../mail/mail_facts.dart';
import '../theme.dart';
import '../widgets/agent_avatar.dart';
import 'approval_format.dart';

part 'approval_card_widgets.dart';
part 'approval_move_widgets.dart';

/// The card bound to the controller: it reads the current view of one approval,
/// so a frame, a 409 re-read or a new answer always reaches the screen.
class ApprovalCardHost extends StatelessWidget {
  const ApprovalCardHost({
    super.key,
    required this.controller,
    required this.approvalId,
    required this.projects,
    this.alfred,
  });

  final ApprovalsController controller;
  final String approvalId;
  final List<ChatProject> projects;
  final ChatSeat? alfred;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        final approval = controller.viewOf(approvalId);
        if (approval == null) return const SizedBox.shrink();
        return ApprovalCard(
          approval: approval,
          agentName: agentTitle(
            approval.seatId,
            projects,
            alfred,
            tool: approval.tool ?? '',
          ),
          projectName: projectNameFor(approval.seatId, projects),
          busy: controller.isBusy(approvalId),
          failure: controller.failureOf(approvalId),
          unchecked: controller.uncheckedOf(approvalId),
          onMoveToggle: (messageId, checked) => controller.setMailChecked(
            approvalId,
            messageId,
            checked: checked,
          ),
          onDecide: (approve) =>
              controller.decide(approvalId, approve: approve),
          onRecheckGrant: () => unawaited(controller.recheckGrant()),
        );
      },
    );
  }
}

/// The compact approval card of the chat timeline (RFC §9): one action, its
/// deadline and the two answers the user may give. It never offers "allow
/// always", and its text is always plain.
class ApprovalCard extends StatefulWidget {
  const ApprovalCard({
    super.key,
    required this.approval,
    required this.agentName,
    required this.busy,
    required this.failure,
    required this.onDecide,
    this.onRecheckGrant,
    this.projectName,
    this.unchecked = const <String>{},
    this.onMoveToggle,
  });

  final ChatApproval approval;
  final String agentName;
  final String? projectName;
  final bool busy;
  final ApprovalFailure? failure;
  final ValueChanged<bool> onDecide;
  final VoidCallback? onRecheckGrant;

  /// The mails of a `mail.move` batch the operator has unchecked.
  final Set<String> unchecked;
  final void Function(String messageId, bool checked)? onMoveToggle;

  @override
  State<ApprovalCard> createState() => _ApprovalCardState();
}

class _ApprovalCardState extends State<ApprovalCard> {
  Timer? _ticker;

  @override
  void initState() {
    super.initState();
    _syncTicker();
  }

  @override
  void didUpdateWidget(covariant ApprovalCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    _syncTicker();
  }

  @override
  void dispose() {
    _ticker?.cancel();
    super.dispose();
  }

  /// The countdown only moves while the answer is still open.
  void _syncTicker() {
    final open = widget.approval.status == 'pending';
    if (open && _ticker == null) {
      _ticker = Timer.periodic(
        const Duration(seconds: 1),
        (_) => setState(() {}),
      );
    } else if (!open) {
      _ticker?.cancel();
      _ticker = null;
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final text = context.text;
    final approval = widget.approval;
    final left = secondsLeft(approval.expiresAt, DateTime.now());
    final closed = _closedStatus(left);
    final detail = approval.question ?? approval.command;
    // The host's generic summary («command Bash») is never shown as the detail.
    final line = detail ?? 'Falta el detalle del comando';
    final place = [
      widget.projectName,
      shortCwd(approval.cwd),
    ].whereType<String>().join(' · ');
    return Container(
      padding: const EdgeInsets.all(Spacing.lg),
      decoration: BoxDecoration(
        color: colors.raised,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: colors.outlineSoft),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              AgentAvatar(
                name: widget.agentName,
                status: AgentStatus.blocked,
                size: 32,
              ),
              const SizedBox(width: Spacing.sm),
              Expanded(
                child: Text(
                  '${widget.agentName} ${_askLine(approval)}',
                  style: text.title,
                ),
              ),
            ],
          ),
          if (place.isNotEmpty) ...[
            const SizedBox(height: Spacing.xs),
            Text(place, style: text.small),
          ],
          const SizedBox(height: Spacing.md),
          ..._facts(context, line, detail, open: closed == null),
          const SizedBox(height: Spacing.sm),
          Wrap(
            spacing: Spacing.sm,
            runSpacing: Spacing.xs,
            children: [
              if (riskLabel(approval.risk) case final risk?) _Chip(label: risk),
              if (levelLabel(approval.level) case final level?)
                _Chip(label: level),
              if (closed == null && left != null)
                _Chip(
                  label: 'Caduca en ${formatCountdown(left)}',
                  icon: Icons.timer_outlined,
                ),
            ],
          ),
          const SizedBox(height: Spacing.md),
          _footer(context, closed),
        ],
      ),
    );
  }

  /// What the operator reads before answering. A mail action shows its facts
  /// instead of a command; the host's generic summary is never the detail.
  List<Widget> _facts(
    BuildContext context,
    String line,
    String? detail, {
    required bool open,
  }) {
    final approval = widget.approval;
    if (approval.mailSend case final send?) {
      return [MailSendFacts(details: send)];
    }
    if (approval.mailAttach case final attach?) {
      return [MailAttachFacts(details: attach)];
    }
    if (approval.mailMove case final move?) {
      return [
        _MoveBatch(
          items: move.items,
          unchecked: widget.unchecked,
          onToggle: open ? widget.onMoveToggle : null,
        ),
      ];
    }
    if (_mailFactsMissing) {
      return [
        Text(
          'No llegaron los datos del correo. Revísalos en la bandeja antes de decidir.',
          style: context.text.bodySecondary,
        ),
      ];
    }
    return [
      _CommandBox(line: line, plain: approval.question != null),
      if (detail != null &&
          approval.summary.isNotEmpty &&
          approval.summary != line) ...[
        const SizedBox(height: Spacing.xs),
        Text(approval.summary, style: context.text.bodySecondary),
      ],
    ];
  }

  /// A mail action whose facts have not arrived. The operator must not approve
  /// what the card does not show, so «Permitir una vez» waits for them.
  bool get _mailFactsMissing {
    final approval = widget.approval;
    return (approval.tool ?? '').startsWith('mail.') &&
        approval.mailSend == null &&
        approval.mailAttach == null &&
        approval.mailMove == null;
  }

  /// A mail batch approves only while one of its mails stays checked.
  bool get _approvable {
    final approval = widget.approval;
    return approval.mailMove == null ||
        canApproveMove(approval, widget.unchecked);
  }

  /// The label of the approve button: a batch says how many mails it moves.
  String get _approveLabel {
    final approval = widget.approval;
    if (approval.mailMove == null) return 'Permitir una vez';
    final total = moveItems(approval).length;
    return 'Mover los seleccionados '
        '(${checkedCount(approval, widget.unchecked)} de $total)';
  }

  /// The request in the title, after the agent's name.
  String _askLine(ChatApproval approval) => switch (approval.tool) {
    'mail.send' => 'quiere enviar un correo',
    'mail.attach_path' => 'quiere adjuntar un archivo',
    'mail.move' => 'quiere mover correos',
    _ => 'necesita permiso',
  };

  /// A status that ends the card, or null while the answer is still open.
  String? _closedStatus(int? left) {
    switch (widget.approval.status) {
      case 'approved':
        return 'Permitida una vez';
      case 'denied':
        return 'Denegada';
      case 'expired':
        return 'Caducada';
      case 'consumed':
        return 'Permitida y ejecutada';
    }
    // Past its deadline, the host will say `expired` on the next read.
    if (left != null && left <= 0) return 'Caducada';
    return null;
  }

  Widget _footer(BuildContext context, String? closed) {
    final text = context.text;
    final colors = context.colors;
    final failure = widget.failure;
    if (closed != null) {
      return Row(
        children: [
          Icon(
            widget.approval.status == 'denied'
                ? Icons.block
                : Icons.check_circle_outline,
            size: 18,
            color: colors.textSecondary,
          ),
          const SizedBox(width: Spacing.sm),
          Text(closed, style: text.bodySecondary),
        ],
      );
    }
    if (failure?.kind == ApprovalFailureKind.needsGrant) {
      return _GrantNotice(
        command: failure?.command ?? '',
        onRecheck: widget.onRecheckGrant,
      );
    }
    if (failure?.kind == ApprovalFailureKind.alreadyDecided) {
      return Text('Ya la decidió otro dispositivo.', style: text.bodySecondary);
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (failure != null) ...[
          Text('No se pudo enviar. Vuelve a intentarlo.', style: text.small),
          const SizedBox(height: Spacing.sm),
        ],
        Row(
          children: [
            Expanded(
              child: FilledButton(
                style: FilledButton.styleFrom(minimumSize: const Size(0, 48)),
                onPressed: widget.busy || _mailFactsMissing || !_approvable
                    ? null
                    : () => widget.onDecide(true),
                child: widget.busy
                    ? const SizedBox.square(
                        dimension: 18,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : Text(_approveLabel),
              ),
            ),
            const SizedBox(width: Spacing.sm),
            Expanded(
              child: OutlinedButton(
                style: OutlinedButton.styleFrom(minimumSize: const Size(0, 48)),
                onPressed: widget.busy ? null : () => widget.onDecide(false),
                child: const Text('Denegar'),
              ),
            ),
          ],
        ),
      ],
    );
  }
}
