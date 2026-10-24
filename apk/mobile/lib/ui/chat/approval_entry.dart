import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';

import '../../core/state/approvals_controller.dart';
import '../theme.dart';
import 'approval_settings_screen.dart';

/// Opens Ajustes → Aprobaciones from the chat header.
void openApprovalSettings(
  BuildContext context, {
  required ApprovalsController approvals,
  required String? conversationId,
  required List<ChatProject> projects,
}) {
  Navigator.of(context).push(
    MaterialPageRoute<void>(
      builder: (_) => ApprovalSettingsScreen(
        approvals: approvals,
        conversationId: conversationId,
        projects: projects,
      ),
    ),
  );
}

/// The shield of the chat header, with the approvals waiting for this user.
class ApprovalsHeaderButton extends StatelessWidget {
  const ApprovalsHeaderButton({
    super.key,
    required this.pending,
    required this.onPressed,
  });

  final int pending;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    return IconButton(
      tooltip: 'Aprobaciones',
      onPressed: onPressed,
      icon: Badge(
        isLabelVisible: pending > 0,
        label: Text('$pending'),
        child: const Icon(Icons.shield_outlined),
      ),
      color: context.colors.textSecondary,
    );
  }
}
