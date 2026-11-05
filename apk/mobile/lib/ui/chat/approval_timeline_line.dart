import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/approvals/model.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';

import '../../core/state/approvals_controller.dart';
import '../theme.dart';
import 'approval_format.dart';

/// One timeline line for an approval the user did not have to decide, such as
/// «Ernesto aprobó: cargo test». It is a line, not a card: no push, no badge.
/// Tapping it opens the chain of levels and the audit.
class ApprovalTimelineLine extends StatelessWidget {
  const ApprovalTimelineLine({
    super.key,
    required this.entry,
    required this.controller,
    required this.conversationId,
    required this.projects,
    this.alfred,
  });

  final ChatEntry entry;
  final ApprovalsController? controller;
  final String? conversationId;
  final List<ChatProject> projects;
  final ChatSeat? alfred;

  @override
  Widget build(BuildContext context) {
    final text = context.text;
    final colors = context.colors;
    final summary = entry.summary?.trim() ?? '';
    return InkWell(
      onTap: () => showApprovalDetail(
        context,
        entry: entry,
        controller: controller,
        conversationId: conversationId,
        projects: projects,
        alfred: alfred,
      ),
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 48),
        child: Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: Spacing.lg,
            vertical: Spacing.sm,
          ),
          child: Row(
            children: [
              Icon(
                _iconFor(entry.status),
                size: 18,
                color: colors.textMuted,
                semanticLabel: 'Aprobación',
              ),
              const SizedBox(width: Spacing.sm),
              Expanded(
                child: Text(
                  summary.isEmpty ? 'Aprobación resuelta' : summary,
                  style: text.small,
                ),
              ),
              Icon(Icons.chevron_right, size: 18, color: colors.textMuted),
            ],
          ),
        ),
      ),
    );
  }
}

IconData _iconFor(String? status) => switch (status) {
  'denied' => Icons.block,
  'expired' => Icons.timer_off_outlined,
  _ => Icons.check_circle_outline,
};

/// The detail of a decided approval: who held each level and the audit rows.
Future<void> showApprovalDetail(
  BuildContext context, {
  required ChatEntry entry,
  required ApprovalsController? controller,
  required String? conversationId,
  required List<ChatProject> projects,
  ChatSeat? alfred,
}) {
  return showModalBottomSheet<void>(
    context: context,
    showDragHandle: true,
    isScrollControlled: true,
    builder: (context) => _ApprovalDetail(
      entry: entry,
      controller: controller,
      conversationId: conversationId,
      projects: projects,
      alfred: alfred,
    ),
  );
}

class _ApprovalDetail extends StatefulWidget {
  const _ApprovalDetail({
    required this.entry,
    required this.controller,
    required this.conversationId,
    required this.projects,
    this.alfred,
  });

  final ChatEntry entry;
  final ApprovalsController? controller;
  final String? conversationId;
  final List<ChatProject> projects;
  final ChatSeat? alfred;

  @override
  State<_ApprovalDetail> createState() => _ApprovalDetailState();
}

class _ApprovalDetailState extends State<_ApprovalDetail> {
  late final Future<ChatApproval?> _record = _load();

  Future<ChatApproval?> _load() {
    final id = widget.entry.approvalId;
    final controller = widget.controller;
    if (id == null || controller == null) return Future.value(null);
    return controller.fetchDecided(
      approvalId: id,
      status: widget.entry.status ?? 'approved',
      conversationId: widget.conversationId,
    );
  }

  @override
  Widget build(BuildContext context) {
    final text = context.text;
    final entry = widget.entry;
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(
          Spacing.lg,
          0,
          Spacing.lg,
          Spacing.lg,
        ),
        child: FutureBuilder<ChatApproval?>(
          future: _record,
          builder: (context, snapshot) {
            final record = snapshot.data;
            final waiting = snapshot.connectionState != ConnectionState.done;
            return SingleChildScrollView(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text('Aprobación', style: text.title),
                  const SizedBox(height: Spacing.xs),
                  Text(entry.summary ?? '', style: text.bodySecondary),
                  const SizedBox(height: Spacing.lg),
                  Text('Quién decidió', style: text.label),
                  const SizedBox(height: Spacing.sm),
                  Text(_decisionLine(record, entry), style: text.body),
                  const SizedBox(height: Spacing.lg),
                  Text('Auditoría', style: text.label),
                  const SizedBox(height: Spacing.sm),
                  if (waiting)
                    const Center(child: CircularProgressIndicator())
                  else if (record == null)
                    Text(
                      'No se pudo cargar el detalle.',
                      style: text.bodySecondary,
                    )
                  else if (record.audit.isEmpty)
                    Text('Sin filas de auditoría.', style: text.bodySecondary)
                  else
                    for (final row in record.audit)
                      Padding(
                        padding: const EdgeInsets.only(bottom: Spacing.xs),
                        child: Text(
                          '${row.decision} · ${row.decidedAt ?? ''}'
                          '${row.deviceId == null ? '' : ' · ${row.deviceId}'}',
                          style: text.small,
                        ),
                      ),
                ],
              ),
            );
          },
        ),
      ),
    );
  }

  /// Who decided, in one plain sentence (no names of the approval levels).
  String _decisionLine(ChatApproval? record, ChatEntry entry) {
    final level = record?.level ?? entry.level ?? 'policy';
    if (level == 'policy') return 'Lo decidió una regla de tu configuración.';
    if (_decidedByUser(record, entry)) return 'Lo decidiste tú.';
    return 'Lo decidió ${_deciderName(record, entry)}.';
  }

  bool _decidedByUser(ChatApproval? record, ChatEntry entry) {
    final kind = record?.deciderKind ?? entry.deciderKind;
    return kind == 'device' || (record?.level ?? entry.level) == 'user';
  }

  String _deciderName(ChatApproval? record, ChatEntry entry) {
    final id = record?.deciderId ?? entry.deciderId;
    if (id == null) return 'alguien';
    return seatFor(id, widget.projects, widget.alfred)?.name ?? id;
  }
}
