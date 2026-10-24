import 'dart:async';

import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/approvals/error.dart';
import 'package:herdr_notify/src/rust/api/approvals/model.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';

import '../../core/state/approvals_controller.dart';
import '../theme.dart';
import 'approval_format.dart';

/// The card bound to the controller: it reads the current view of one approval,
/// so a frame, a 409 re-read or a new answer always reaches the screen.
class ApprovalCardHost extends StatelessWidget {
  const ApprovalCardHost({
    super.key,
    required this.controller,
    required this.approvalId,
    required this.projects,
  });

  final ApprovalsController controller;
  final String approvalId;
  final List<ChatProject> projects;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        final approval = controller.viewOf(approvalId);
        if (approval == null) return const SizedBox.shrink();
        return ApprovalCard(
          approval: approval,
          agentName: agentTitle(approval.seatId, projects),
          projectName: projectNameFor(approval.seatId, projects),
          busy: controller.isBusy(approvalId),
          failure: controller.failureOf(approvalId),
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
  });

  final ChatApproval approval;
  final String agentName;
  final String? projectName;
  final bool busy;
  final ApprovalFailure? failure;
  final ValueChanged<bool> onDecide;
  final VoidCallback? onRecheckGrant;

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
              Icon(
                Icons.shield_outlined,
                size: 20,
                color: colors.accent,
                semanticLabel: 'Permiso',
              ),
              const SizedBox(width: Spacing.sm),
              Expanded(
                child: Text(
                  '${widget.agentName} necesita permiso',
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
          _CommandBox(line: line, plain: approval.question != null),
          if (detail != null &&
              approval.summary.isNotEmpty &&
              approval.summary != line) ...[
            const SizedBox(height: Spacing.xs),
            Text(approval.summary, style: text.bodySecondary),
          ],
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
          Text(
            'No se pudo enviar. Vuelve a intentarlo.',
            style: text.small.copyWith(color: colors.statusBlocked),
          ),
          const SizedBox(height: Spacing.sm),
        ],
        Row(
          children: [
            Expanded(
              child: FilledButton(
                style: FilledButton.styleFrom(minimumSize: const Size(0, 48)),
                onPressed: widget.busy ? null : () => widget.onDecide(true),
                child: widget.busy
                    ? const SizedBox.square(
                        dimension: 18,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Text('Permitir una vez'),
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

/// The monospaced command, or the host's question in body text when it asked.
class _CommandBox extends StatelessWidget {
  const _CommandBox({required this.line, required this.plain});

  final String line;
  final bool plain;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(
        horizontal: Spacing.md,
        vertical: Spacing.sm,
      ),
      decoration: BoxDecoration(
        color: colors.surface,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Text(line, style: plain ? context.text.body : context.text.mono),
    );
  }
}

/// The 403 without `APPROVE`: the canonical host line, selectable to copy, and
/// the recheck for a grant made since.
class _GrantNotice extends StatelessWidget {
  const _GrantNotice({required this.command, this.onRecheck});

  final String command;
  final VoidCallback? onRecheck;

  @override
  Widget build(BuildContext context) {
    final text = context.text;
    final colors = context.colors;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'Este dispositivo no puede aprobar. En el host, ejecuta:',
          style: text.bodySecondary,
        ),
        const SizedBox(height: Spacing.xs),
        Container(
          width: double.infinity,
          padding: const EdgeInsets.all(Spacing.sm),
          decoration: BoxDecoration(
            color: colors.surface,
            borderRadius: BorderRadius.circular(8),
          ),
          child: SelectableText(command, style: text.mono),
        ),
        if (onRecheck != null) ...[
          const SizedBox(height: Spacing.sm),
          OutlinedButton(
            style: OutlinedButton.styleFrom(minimumSize: const Size(0, 48)),
            onPressed: onRecheck,
            child: const Text('Ya lo he concedido'),
          ),
        ],
      ],
    );
  }
}

class _Chip extends StatelessWidget {
  const _Chip({required this.label, this.icon});

  final String label;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: Spacing.sm,
        vertical: Spacing.xs,
      ),
      decoration: BoxDecoration(
        color: colors.surface3,
        borderRadius: BorderRadius.circular(999),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (icon != null) ...[
            Icon(icon, size: 14, color: colors.textSecondary),
            const SizedBox(width: Spacing.xs),
          ],
          Text(label, style: context.text.label),
        ],
      ),
    );
  }
}
