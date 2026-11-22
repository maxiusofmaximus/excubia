import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:excubia/src/rust/api/approvals/error.dart';
import 'package:excubia/src/rust/api/approvals/model.dart';
import 'package:excubia/src/rust/api/approvals/plan.dart';
import 'package:excubia/src/rust/api/approvals/reducer.dart';
import 'package:excubia/src/rust/api/chat/model.dart';
import 'package:excubia/src/rust/api/chat/plan.dart' show newChatClientId;

import '../api/approval_find.dart';
import '../api/chat_transport.dart';
import '../api/host_client.dart';
import '../api/plan_runner.dart';
import 'approval_move.dart';

/// What the chat shows of approvals: one view per approval id, the mode
/// settings and the decisions in flight. The host stays the only writer: a
/// card changes to decided only when the host has answered.
class ApprovalsController extends ChangeNotifier {
  ApprovalsController({required this._transport, required this._deviceId});

  final ChatTransport _transport;
  final String _deviceId;
  final ApprovalReducer _reducer = newApprovalReducer();
  // One client_id per approval while a retry may still come: the host answers
  // the same id with the same answer, so a retry never decides twice.
  final Map<String, _Attempt> _attempts = <String, _Attempt>{};
  // The mails a user unchecked in a mail.move batch. Kept here, not in the card,
  // so a refresh or a rotation does not check them again.
  final Map<String, Set<String>> _unchecked = <String, Set<String>>{};
  final Map<String, ApprovalFailure> _failures = <String, ApprovalFailure>{};
  final Set<String> _busy = <String>{};
  ApprovalSettings? _settings;
  ApprovalFailure? _settingsFailure;
  bool _disposed = false;

  ApprovalSettings? get settings => _settings;
  ApprovalFailure? get settingsFailure => _settingsFailure;

  /// No `APPROVE` grant: the settings controls stay disabled.
  bool get settingsDenied =>
      _settingsFailure?.kind == ApprovalFailureKind.needsGrant;

  bool isBusy(String approvalId) => _busy.contains(approvalId);

  ApprovalFailure? failureOf(String approvalId) => _failures[approvalId];

  /// The current view of one approval, whatever its status.
  ChatApproval? viewOf(String approvalId) {
    for (final view in _reducer.snapshot(conversationId: null)) {
      if (view.approvalId == approvalId) return view;
    }
    return null;
  }

  /// The pending approvals this user must decide, in one conversation or all.
  List<ChatApproval> userPending({String? conversationId}) =>
      _reducer.userPending(conversationId: conversationId);

  /// The user's approvals that belong to no conversation, pending or closed in
  /// this session. Only the inbox shows them: no timeline can hold them.
  List<ChatApproval> inboxUnassigned() => _reducer.userInboxUnassigned();

  /// Reads the operator inbox and the settings. A pending card that the inbox
  /// no longer lists was decided somewhere else, so its state is read again.
  Future<void> refresh() async {
    try {
      final body = await runPlan(_transport, planApprovalInbox());
      final rows = parseApprovalInbox(json: body) ?? const <ChatApproval>[];
      final listed = <String>{for (final row in rows) row.approvalId};
      for (final row in rows) {
        _reducer.acceptRecord(record: row);
      }
      await Future.wait([
        for (final stale in _reducer.userPending(conversationId: null))
          if (!listed.contains(stale.approvalId))
            readApproval(stale.approvalId),
      ]);
    } catch (_) {
      // Keep the last known cards; the next refresh or frame corrects them.
    }
    await loadSettings();
    _notify();
  }

  /// One `chat.approval` frame. Other chat events belong to the chat.
  void onChatEvent(ChatEvent event) {
    if (event.event != 'chat.approval') return;
    final changed = _reducer.acceptEvent(event: event);
    // A frame carries no command or cwd: the inbox does, so a new pending
    // approval triggers a read of the inbox.
    if (event.status == 'pending') unawaited(refresh());
    if (changed) _notify();
  }

  /// The mails checked off in one `mail.move` batch are the ones the operator
  /// keeps: this is what the card shows and what a decision leaves out.
  Set<String> uncheckedOf(String approvalId) =>
      _unchecked[approvalId] ?? const <String>{};

  /// Checks or unchecks one mail of a batch. Only an approval sends the result.
  void setMailChecked(
    String approvalId,
    String messageId, {
    required bool checked,
  }) {
    final unchecked = {...uncheckedOf(approvalId)};
    if (checked) {
      unchecked.remove(messageId);
    } else {
      unchecked.add(messageId);
    }
    _unchecked[approvalId] = unchecked;
    _notify();
  }

  /// Answers one approval once. A retry after a network failure reuses the
  /// same `client_id`.
  Future<void> decide(String approvalId, {required bool approve}) async {
    if (_busy.contains(approvalId)) return;
    final exclude = _exclusionsFor(approvalId, approve: approve);
    if (exclude == null) return;
    _busy.add(approvalId);
    _failures.remove(approvalId);
    _notify();
    final clientId = _clientIdFor(
      approvalId,
      approve: approve,
      exclude: exclude,
    );
    try {
      final body = await runPlan(
        _transport,
        planApprovalDecision(
          approvalId: approvalId,
          approve: approve,
          clientId: clientId,
          exclude: exclude,
        ),
      );
      final record = parseApprovalRecord(json: body);
      if (record != null) _reducer.acceptRecord(record: record);
      _attempts.remove(approvalId);
    } catch (error) {
      final failure = _explain(error);
      _failures[approvalId] = failure;
      // A refusal does not change with a retry, so its client_id is spent.
      if (failure.kind != ApprovalFailureKind.other) {
        _attempts.remove(approvalId);
      }
      if (failure.kind == ApprovalFailureKind.alreadyDecided ||
          failure.kind == ApprovalFailureKind.expired) {
        await readApproval(approvalId);
      }
    } finally {
      _busy.remove(approvalId);
      _notify();
    }
  }

  /// Asks the host whether this device holds `APPROVE` now. The decision route
  /// checks the grant before it looks at the id: an unknown id answers 403
  /// without the grant and 404 with it, so nothing is decided either way.
  /// Returns true when the grant is there; the stale 403 notices then clear.
  Future<bool> recheckGrant() async {
    try {
      await runPlan(
        _transport,
        planApprovalDecision(
          approvalId: _grantProbeId,
          approve: false,
          clientId: newChatClientId(),
          exclude: const [],
        ),
      );
      return false;
    } on HostError catch (error) {
      if (error.status != 404) return false;
    } catch (_) {
      return false;
    }
    _failures.removeWhere(
      (_, failure) => failure.kind == ApprovalFailureKind.needsGrant,
    );
    if (settingsDenied) _settingsFailure = null;
    _notify();
    return true;
  }

  /// The state and deadline of one approval, from the inbox (see [findApproval]).
  Future<void> readApproval(String approvalId) async {
    try {
      final record = await findApproval(_transport, approvalId);
      if (record != null) _reducer.acceptRecord(record: record);
    } catch (_) {
      // The card keeps its last state; its deadline still closes it.
    }
    _notify();
  }

  /// The full record of a decided approval (levels and audit), for the
  /// timeline detail. Null when the host does not list it.
  Future<ChatApproval?> fetchDecided({
    required String approvalId,
    required String status,
    String? conversationId,
  }) async {
    // An approved batch is consumed once the host applies it, so the card's
    // own status is only the first place to look.
    for (final asked in {status, 'consumed', 'approved'}) {
      try {
        final body = await runPlan(
          _transport,
          planApprovalInbox(conversationId: conversationId, status: asked),
        );
        final rows = parseApprovalInbox(json: body) ?? const <ChatApproval>[];
        for (final row in rows) {
          if (row.approvalId != approvalId) continue;
          _reducer.acceptRecord(record: row);
          _notify();
          return row;
        }
      } catch (_) {
        // The detail shows its own message when there is no record.
      }
    }
    return null;
  }

  Future<void> loadSettings() async {
    try {
      final body = await runPlan(_transport, planApprovalSettingsRead());
      _settings = parseApprovalSettings(json: body) ?? _settings;
      _settingsFailure = null;
    } catch (error) {
      _settingsFailure = _explain(error);
    }
    _notify();
  }

  /// Sets the global mode, or one project's override when [projectId] is given.
  Future<bool> setMode({required String mode, String? projectId}) async {
    if (settingsDenied) return false;
    try {
      final body = await runPlan(
        _transport,
        planApprovalSettingsWrite(
          mode: mode,
          projectId: projectId,
          clientId: newChatClientId(),
        ),
      );
      _settings = parseApprovalSettings(json: body) ?? _settings;
      _settingsFailure = null;
      _notify();
      return true;
    } catch (error) {
      _settingsFailure = _explain(error);
      _notify();
      return false;
    }
  }

  /// The exclusions a decision sends: only an approval of `mail.move` has them.
  /// Null when every mail of the batch is unchecked, which the card never sends.
  List<String>? _exclusionsFor(String approvalId, {required bool approve}) {
    final view = viewOf(approvalId);
    if (!approve || view == null || !isMailMove(view)) return const [];
    final unchecked = uncheckedOf(approvalId);
    if (!canApproveMove(view, unchecked)) return null;
    return excludeFor(view, unchecked);
  }

  /// A retry reuses its `client_id` only for the same decision and the same
  /// exclusions; any other answer is a new decision with a new id.
  String _clientIdFor(
    String approvalId, {
    required bool approve,
    required List<String> exclude,
  }) {
    final previous = _attempts[approvalId];
    if (previous != null &&
        previous.approve == approve &&
        listEquals(previous.exclude, exclude)) {
      return previous.clientId;
    }
    final attempt = _Attempt(
      newChatClientId(),
      approve: approve,
      exclude: exclude,
    );
    _attempts[approvalId] = attempt;
    return attempt.clientId;
  }

  ApprovalFailure _explain(Object error) {
    if (error is HostError) {
      return explainApprovalFailure(
        status: error.status,
        body: error.body,
        deviceId: _deviceId,
      );
    }
    if (error is PlanRefused) return _refused;
    return _offline;
  }

  // Never a real approval: the grant probe only reads the 403/404 answer.
  static const String _grantProbeId = 'apr_grant_probe';

  static const ApprovalFailure _refused = ApprovalFailure(
    kind: ApprovalFailureKind.other,
    code: 'refused',
  );

  static const ApprovalFailure _offline = ApprovalFailure(
    kind: ApprovalFailureKind.other,
    code: 'offline',
  );

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}

/// One decision that a retry may repeat, with the exact answer it carries.
class _Attempt {
  const _Attempt(this.clientId, {required this.approve, required this.exclude});

  final String clientId;
  final bool approve;
  final List<String> exclude;
}
