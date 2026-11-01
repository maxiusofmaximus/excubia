import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/approvals/model.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';

import '../../core/state/approvals_controller.dart';
import '../theme.dart';
import 'approval_card.dart';

/// The operator inbox above the chat: approvals that belong to no conversation,
/// so no timeline can show them. The pending ones come first, one card at a time
/// and scrollable, so their buttons are never cut. The answered ones wait in
/// «Recientes», folded by default. Renders nothing when there is none.
class ApprovalInbox extends StatefulWidget {
  const ApprovalInbox({
    super.key,
    required this.controller,
    required this.projects,
    this.alfred,
  });

  final ApprovalsController controller;
  final List<ChatProject> projects;
  final ChatSeat? alfred;

  @override
  State<ApprovalInbox> createState() => _ApprovalInboxState();
}

class _ApprovalInboxState extends State<ApprovalInbox> {
  int _index = 0;
  bool _recentOpen = false;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.controller,
      builder: (context, _) {
        final rows = widget.controller.inboxUnassigned();
        if (rows.isEmpty) return const SizedBox.shrink();
        final pending = [
          for (final row in rows)
            if (row.status == 'pending') row,
        ];
        final recent = [
          for (final row in rows)
            if (row.status != 'pending') row,
        ];
        return Padding(
          padding: const EdgeInsets.fromLTRB(
            Spacing.lg,
            Spacing.sm,
            Spacing.lg,
            0,
          ),
          // Capped, so a long list never takes half the chat; the content scrolls.
          child: ConstrainedBox(
            constraints: BoxConstraints(
              maxHeight: MediaQuery.sizeOf(context).height * 0.4,
            ),
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (pending.isNotEmpty) ..._pendingSection(pending),
                  if (recent.isNotEmpty) _recentSection(recent),
                ],
              ),
            ),
          ),
        );
      },
    );
  }

  List<Widget> _pendingSection(List<ChatApproval> pending) {
    final index = _index.clamp(0, pending.length - 1);
    final approval = pending[index];
    return [
      Row(
        children: [
          Expanded(
            child: Text(
              pending.length == 1
                  ? 'Pendientes de ti'
                  : 'Pendientes de ti · ${index + 1} de ${pending.length}',
              style: context.text.label,
            ),
          ),
          if (pending.length > 1) ...[
            IconButton(
              tooltip: 'Anterior',
              icon: const Icon(Icons.chevron_left),
              onPressed: index > 0
                  ? () => setState(() => _index = index - 1)
                  : null,
            ),
            IconButton(
              tooltip: 'Siguiente',
              icon: const Icon(Icons.chevron_right),
              onPressed: index < pending.length - 1
                  ? () => setState(() => _index = index + 1)
                  : null,
            ),
          ],
        ],
      ),
      const SizedBox(height: Spacing.sm),
      ApprovalCardHost(
        controller: widget.controller,
        approvalId: approval.approvalId,
        projects: widget.projects,
        alfred: widget.alfred,
      ),
      const SizedBox(height: Spacing.sm),
    ];
  }

  Widget _recentSection(List<ChatApproval> recent) {
    final colors = context.colors;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        InkWell(
          onTap: () => setState(() => _recentOpen = !_recentOpen),
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: Spacing.sm),
            child: Row(
              children: [
                Icon(
                  _recentOpen ? Icons.expand_less : Icons.expand_more,
                  size: 18,
                  color: colors.textSecondary,
                ),
                const SizedBox(width: Spacing.xs),
                Text('Recientes · ${recent.length}', style: context.text.label),
              ],
            ),
          ),
        ),
        if (_recentOpen)
          for (final approval in recent)
            Padding(
              padding: const EdgeInsets.only(bottom: Spacing.sm),
              child: ApprovalCardHost(
                controller: widget.controller,
                approvalId: approval.approvalId,
                projects: widget.projects,
                alfred: widget.alfred,
              ),
            ),
      ],
    );
  }
}
