import 'dart:async';

import 'package:flutter/material.dart';
import 'package:excubia/src/rust/api/approvals/error.dart';
import 'package:excubia/src/rust/api/approvals/model.dart';
import 'package:excubia/src/rust/api/chat/model_team.dart';
import 'package:excubia/src/rust/api/state.dart';

import '../../core/state/approval_move.dart';
import '../../core/state/approvals_controller.dart';
import '../mail/mail_facts.dart';
import '../theme.dart';
import '../widgets/agent_avatar.dart';
import '../widgets/comic/comic_button.dart';
import '../widgets/comic/comic_slab.dart';
import '../widgets/comic/comic_tag.dart';
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
    this.unchecked = const <String>{},
    this.onMoveToggle,
  });

  final ChatApproval approval;
  final String agentName;
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

  /// The system's «reducir animaciones», read again whenever it changes.
  bool _reduceMotion = false;

  @override
  void initState() {
    super.initState();
    _syncTicker();
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _reduceMotion = MediaQuery.disableAnimationsOf(context);
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
    final folder = shortCwd(approval.cwd);
    // The stamp flips between two tones once a second while the answer is open,
    // on the card's own clock; with reduced motion it holds the accent.
    final stampPale =
        closed == null && !_reduceMotion && DateTime.now().second.isOdd;
    return ComicSlab(
      color: colors.deep,
      cut: true,
      padding: const EdgeInsets.all(Spacing.lg),
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
              if (closed == null) ...[
                const SizedBox(width: Spacing.sm),
                ComicTag(
                  label: '¡Tu OK!',
                  caps: false,
                  tone: stampPale ? ComicTagTone.pale : ComicTagTone.hot,
                ),
              ],
            ],
          ),
          if (folder != null) ...[
            const SizedBox(height: Spacing.xs),
            Text('Carpeta: $folder', style: text.small),
          ],
          if (approval.risk == 'high' && isShellTool(approval.tool)) ...[
            const SizedBox(height: Spacing.xs),
            Text('Puede cambiar o borrar archivos.', style: text.small),
          ],
          const SizedBox(height: Spacing.md),
          ..._facts(context),
          if (_technical() case final technical?) ...[
            const SizedBox(height: Spacing.sm),
            technical,
          ],
          const SizedBox(height: Spacing.sm),
          Wrap(
            spacing: Spacing.sm,
            runSpacing: Spacing.xs,
            children: [
              // A card that waits for days (a file for a draft) shows no clock.
              if (closed == null && left != null && left < 3600)
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

  /// What the operator reads before answering. A mail action shows its facts;
  /// a question shows its text. A command waits in «Detalles técnicos».
  List<Widget> _facts(BuildContext context) {
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
          onToggle: _open ? widget.onMoveToggle : null,
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
    if (approval.question case final question?) {
      return [_CommandBox(line: question, plain: true)];
    }
    return const [];
  }

  /// The command and the host's tool name, folded under «Detalles técnicos» and
  /// closed. Null when no command came, so no empty section shows.
  Widget? _technical() {
    final approval = widget.approval;
    if (approval.question != null) return null;
    final command = approval.command;
    if (command == null || command.isEmpty) return null;
    return _TechnicalDetails(command: command, summary: approval.summary);
  }

  /// Whether the card is still open for an answer.
  bool get _open => widget.approval.status == 'pending';

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

  /// The request in the title, after the agent's name: what it wants to do.
  String _askLine(ChatApproval approval) {
    if (approval.question != null) return 'necesita permiso';
    return switch (approval.tool) {
      'mail.send' => 'quiere enviar un correo',
      'mail.attach_path' => 'quiere adjuntar un archivo',
      'mail.move' => 'quiere mover correos',
      _ => switch (actionPhrase(approval.tool, approval.command)) {
        final phrase? => 'quiere $phrase',
        _ => 'necesita permiso',
      },
    };
  }

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
            // The main answer takes the wider share: «Permitir una vez» fits on one line.
            Expanded(
              flex: 3,
              child: ComicButton(
                label: _approveLabel,
                busy: widget.busy,
                onPressed: widget.busy || _mailFactsMissing || !_approvable
                    ? null
                    : () => widget.onDecide(true),
              ),
            ),
            const SizedBox(width: Spacing.sm),
            Expanded(
              flex: 2,
              child: ComicButton(
                label: 'Denegar',
                tone: ComicButtonTone.secondary,
                onPressed: widget.busy ? null : () => widget.onDecide(false),
              ),
            ),
          ],
        ),
      ],
    );
  }
}
