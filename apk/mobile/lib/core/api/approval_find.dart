import '../../src/rust/api/approvals/model.dart';
import '../../src/rust/api/approvals/plan.dart';
import 'chat_transport.dart';
import 'plan_runner.dart';

const List<String> _resolvedStatuses = [
  'approved',
  'denied',
  'expired',
  'consumed',
];

/// The record of one approval, read from the operator inbox. A device cannot
/// read one approval by id: `GET /v1/chat/approvals/{id}` is a hook route and
/// answers a signed device 401. So the pending inbox is searched first, then
/// the resolved statuses, as the Desktop does. Null when no list has it.
Future<ChatApproval?> findApproval(
  ChatTransport transport,
  String approvalId,
) async {
  final pending = await _listedIn(transport, approvalId);
  if (pending != null) return pending;
  final resolved = await Future.wait([
    for (final status in _resolvedStatuses)
      _listedIn(transport, approvalId, status: status),
  ]);
  for (final found in resolved) {
    if (found != null) return found;
  }
  return null;
}

Future<ChatApproval?> _listedIn(
  ChatTransport transport,
  String approvalId, {
  String? status,
}) async {
  final body = await runPlan(transport, planApprovalInbox(status: status));
  final rows = parseApprovalInbox(json: body) ?? const <ChatApproval>[];
  for (final row in rows) {
    if (row.approvalId == approvalId) return row;
  }
  return null;
}
