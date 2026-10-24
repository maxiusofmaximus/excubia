import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';

import '../../core/state/approvals_controller.dart';
import '../theme.dart';
import 'approval_card.dart';

/// The operator inbox above the chat: approvals that belong to no conversation,
/// so no timeline can show them. A closed card stays until the app restarts, so
/// the answer remains readable. Renders nothing when there is none.
class ApprovalInbox extends StatelessWidget {
  const ApprovalInbox({
    super.key,
    required this.controller,
    required this.projects,
  });

  final ApprovalsController controller;
  final List<ChatProject> projects;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        final rows = controller.inboxUnassigned();
        if (rows.isEmpty) return const SizedBox.shrink();
        final open = rows.any((approval) => approval.status == 'pending');
        return Padding(
          padding: const EdgeInsets.fromLTRB(
            Spacing.lg,
            Spacing.sm,
            Spacing.lg,
            0,
          ),
          child: ConstrainedBox(
            constraints: BoxConstraints(
              maxHeight: MediaQuery.sizeOf(context).height * 0.5,
            ),
            child: ListView(
              shrinkWrap: true,
              padding: EdgeInsets.zero,
              children: [
                Text(
                  open ? 'Pendientes de ti' : 'Aprobaciones recientes',
                  style: context.text.label,
                ),
                const SizedBox(height: Spacing.sm),
                for (final approval in rows)
                  Padding(
                    padding: const EdgeInsets.only(bottom: Spacing.sm),
                    child: ApprovalCardHost(
                      controller: controller,
                      approvalId: approval.approvalId,
                      projects: projects,
                    ),
                  ),
              ],
            ),
          ),
        );
      },
    );
  }
}
