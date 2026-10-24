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
  });

  final ChatEntry entry;
  final ApprovalsController? controller;
  final String? conversationId;
  final List<ChatProject> projects;

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
    ),
  );
}

/// Levels in the order the approval climbs (RFC §5).
const List<(String, String)> _levels = [
  ('policy', 'Política'),
  ('lead', 'Lead'),
  ('cto', 'Alfred'),
  ('user', 'Tú'),
];

class _ApprovalDetail extends StatefulWidget {
  const _ApprovalDetail({
    required this.entry,
    required this.controller,
    required this.conversationId,
    required this.projects,
  });

  final ChatEntry entry;
  final ApprovalsController? controller;
  final String? conversationId;
  final List<ChatProject> projects;

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
                  Text('Cadena de niveles', style: text.label),
                  const SizedBox(height: Spacing.sm),
                  for (final row in _chain(record, entry))
                    Text(row, style: text.body),
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

  /// One line per level: passed without deciding, the decider, or not reached.
  List<String> _chain(ChatApproval? record, ChatEntry entry) {
    final decided = record?.level ?? entry.level ?? 'policy';
    final decidedAt = _levels.indexWhere((level) => level.$1 == decided);
    final decider = _deciderName(record, entry);
    return [
      for (var i = 0; i < _levels.length; i++)
        '${_levels[i].$2}: ${_stateOf(i, decidedAt, decider)}',
    ];
  }

  String _stateOf(int index, int decidedAt, String decider) {
    if (decidedAt < 0) return 'sin decidir';
    if (index < decidedAt) return 'pasó sin decidir';
    if (index == decidedAt) return 'decidió $decider';
    return 'no llegó';
  }

  String _deciderName(ChatApproval? record, ChatEntry entry) {
    final kind = record?.deciderKind ?? entry.deciderKind;
    final id = record?.deciderId ?? entry.deciderId;
    if (kind == 'device' || (record?.level ?? entry.level) == 'user') {
      return 'tú';
    }
    if (id == null) return 'alguien';
    return seatFor(id, widget.projects)?.name ?? id;
  }
}
