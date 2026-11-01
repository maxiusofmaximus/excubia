import 'dart:async';

import 'package:flutter/foundation.dart';

import '../../src/rust/api/approvals/model.dart';
import '../api/gateway_client.dart';
import '../api/mail_api.dart';
import '../api/plan_runner.dart';

/// Whether an approval covers the draft the operator reads. Any difference in a
/// shown fact means they would approve something other than what they see.
bool sameMailSend(MailDraft draft, MailSendDetails details, String account) {
  if (details.account != account) return false;
  if ((draft.from ?? '') != (details.from ?? '')) return false;
  if (!listEquals(draft.to, details.to)) return false;
  if (!listEquals(draft.cc, details.cc)) return false;
  if (draft.subject != details.subject) return false;
  if (!draft.body.startsWith(details.bodyPreview)) return false;
  if (draft.attachments.length != details.attachments.length) return false;
  for (var i = 0; i < draft.attachments.length; i++) {
    final shown = draft.attachments[i];
    final approved = details.attachments[i];
    if (shown.name != approved.name ||
        shown.size != approved.size ||
        shown.sha256 != approved.sha256) {
      return false;
    }
  }
  return true;
}

/// Where one send stands. Only [MailSendPhase.awaitingApproval] moves to
/// [MailSendPhase.scheduled], and only when the operator approved in the chat.
enum MailSendPhase {
  idle,
  requesting,
  awaitingApproval,
  scheduled,
  checking,
  finished,
}

/// Reviews one draft and drives its send (contract `mail.md`, «Envío»). The
/// app asks for approval, reads the approval, and confirms the send only when
/// the operator approved it. A denied, expired or used approval ends the flow.
/// Nothing is sent without that approval, and a failed step is never retried
/// on its own.
class MailDraftController extends ChangeNotifier {
  MailDraftController(
    this._mail, {
    required this.account,
    required this.draftId,
    this.tick = const Duration(seconds: 1),
    DateTime Function()? clock,
  }) : _clock = clock ?? DateTime.now;

  /// How often the controller reads the host while a send is open.
  final Duration tick;

  final MailApi _mail;
  final String account;
  final String draftId;
  final DateTime Function() _clock;

  /// Host status reads before the controller stops asking and says so.
  static const int _maxChecks = 60;

  MailDraft? _draft;
  String? _loadFailure;
  MailSendPhase _phase = MailSendPhase.idle;
  String? _approvalId;
  DateTime? _approvalEndsAt;
  String? _sendId;
  DateTime? _windowEndsAt;
  String? _notice;
  MailSendFailure? _failure;
  bool _busy = false;
  bool _reading = false;
  bool _disposed = false;
  // The host refuses the same content after a denial until the draft changes.
  bool _refused = false;
  int _ticks = 0;
  int _checks = 0;
  Timer? _timer;

  MailDraft? get draft => _draft;
  String? get loadFailure => _loadFailure;
  MailSendPhase get phase => _phase;
  String? get notice => _notice;

  /// Why the send failed; set only when it ended as `failed`.
  MailSendFailure? get failure => _failure;
  bool get busy => _busy;

  /// The approval the operator answers in the chat; null while no send is open.
  String? get approvalId => _approvalId;

  /// When the open approval expires (120 s on the host).
  DateTime? get approvalEndsAt => _approvalEndsAt;

  /// When the window closes; the countdown reads it.
  DateTime? get windowEndsAt => _windowEndsAt;

  bool get canCancel => _phase == MailSendPhase.scheduled && !_busy;

  /// No send is open or scheduled on the draft. A finished flow that left nothing
  /// scheduled (expired, denied, cancelled) counts.
  bool get _sendClosed =>
      _phase == MailSendPhase.idle ||
      (_phase == MailSendPhase.finished && _sendId == null);

  /// Expired, cancelled and used approvals can be asked again; a denied one only
  /// after the draft changes.
  bool get canRequestSend =>
      _draft != null && !_busy && _sendClosed && !_refused;

  /// The draft can change when no send is open or scheduled on it, or when a
  /// failed send says the message did not leave. An edit asks for approval again.
  bool get canEdit =>
      _draft != null && !_busy && (_sendClosed || (_failure?.safe ?? false));

  /// True once the flow has ended; its timer is stopped and nothing reads the
  /// host any more.
  bool get finished => _phase == MailSendPhase.finished;

  Future<void> load() async {
    final draft = await _run(() => _mail.draft(account, draftId));
    if (draft == null) {
      _loadFailure = _notice;
      _notice = null;
    } else {
      _draft = draft;
      _loadFailure = null;
    }
    _notify();
  }

  /// Saves a change to the draft and reads it again. A send that was waiting for
  /// approval is no longer the one on the host, so the flow goes back to idle.
  Future<bool> update({
    required List<String> to,
    required List<String> cc,
    required String subject,
    required String body,
  }) async {
    if (!canEdit) return false;
    final saved = await _run(
      () => _mail.updateDraft(
        account: account,
        draftId: draftId,
        to: to,
        cc: cc,
        subject: subject,
        body: body,
      ),
    );
    if (saved == null) {
      _notify();
      return false;
    }
    _approvalId = null;
    _approvalEndsAt = null;
    _sendId = null;
    _windowEndsAt = null;
    _refused = false;
    _failure = null;
    _phase = MailSendPhase.idle;
    await load();
    _notice = 'Borrador guardado. Si cambias el borrador hay que aprobar el envío de nuevo.';
    _notify();
    return true;
  }

  /// Asks for approval. The host shows the approval in the chat; nothing else
  /// happens here until the operator decides.
  Future<void> requestSend() async {
    if (!canRequestSend) return;
    _sendId = null;
    _windowEndsAt = null;
    _phase = MailSendPhase.requesting;
    _notify();
    final request = await _run(() => _mail.requestSend(account, draftId));
    if (request == null) {
      _phase = MailSendPhase.idle;
      _notify();
      return;
    }
    _approvalId = request.approvalId;
    _approvalEndsAt = _clock().add(Duration(seconds: request.expiresIn));
    _phase = MailSendPhase.awaitingApproval;
    _startTimer();
    _notify();
  }

  /// Cancels inside the window. If the host cannot be reached, the controller
  /// asks it what happened instead of guessing.
  Future<void> cancel() async {
    if (!canCancel) return;
    final state = await _run(() => _mail.cancelSend(_sendId!));
    if (state == null) {
      _phase = MailSendPhase.checking;
      _checks = 0;
      _startTimer();
      _notify();
      return;
    }
    _settle(state);
  }

  void _startTimer() {
    _timer ??= Timer.periodic(tick, (_) => unawaited(_onTick()));
  }

  void _stopTimer() {
    _timer?.cancel();
    _timer = null;
  }

  Future<void> _onTick() async {
    if (_reading || _disposed) return;
    _ticks++;
    switch (_phase) {
      case MailSendPhase.awaitingApproval:
        if (!_clock().isBefore(_approvalEndsAt!)) {
          _end(_expiredText);
          return;
        }
        // The approval is read every other tick; the countdown needs no host.
        _notify();
        if (_ticks.isEven) await _guarded(_readApproval);
      case MailSendPhase.scheduled:
        if (!_clock().isBefore(_windowEndsAt!)) {
          _phase = MailSendPhase.checking;
          _checks = 0;
        }
        _notify();
      case MailSendPhase.checking:
        await _guarded(_checkSend);
      case MailSendPhase.idle:
      case MailSendPhase.requesting:
      case MailSendPhase.finished:
        break;
    }
  }

  Future<void> _guarded(Future<void> Function() step) async {
    _reading = true;
    try {
      await step();
    } finally {
      _reading = false;
    }
  }

  /// Null from [MailApi.approval] means no list has it yet, or the read failed
  /// (its notice is set): the flow keeps waiting, and the clock ends it.
  Future<void> _readApproval() async {
    final approval = await _run(() => _mail.approval(_approvalId!));
    if (approval == null) return;
    switch (approval.status) {
      case 'approved':
        await _confirm();
      case 'denied':
        _refused = true;
        _end(_deniedText);
      case 'expired':
        _end(_expiredText);
      case 'consumed':
        _end(_usedText);
    }
  }

  /// Runs only after the approval read says `approved`: the one path that
  /// schedules the send.
  Future<void> _confirm() async {
    _stopTimer();
    _phase = MailSendPhase.requesting;
    _notify();
    final window = await _run(
      () => _mail.confirmSend(
        account: account,
        draftId: draftId,
        approvalId: _approvalId!,
      ),
    );
    if (window == null) {
      _end(_notice ?? mailCodeText(null));
      return;
    }
    _sendId = window.sendId;
    _windowEndsAt = _clock().add(Duration(seconds: window.windowSeconds));
    _phase = MailSendPhase.scheduled;
    _startTimer();
    _notify();
  }

  Future<void> _checkSend() async {
    _checks++;
    if (_checks > _maxChecks) {
      _end(
        'Todavía no sabemos si el correo salió. '
        'Revisa Enviados antes de volver a intentarlo.',
      );
      return;
    }
    final state = await _run(() => _mail.sendState(_sendId!));
    if (state != null) _settle(state);
  }

  void _settle(MailSendStatus status) {
    final state = status.state;
    if (!state.isFinal) return;
    // Cancelled means nothing goes out, so the draft is free to send again.
    if (state == MailSendState.cancelled) _sendId = null;
    _failure = status.failure;
    _end(switch (state) {
      MailSendState.sent => 'Correo enviado.',
      MailSendState.cancelled => 'Envío cancelado. No se ha enviado nada.',
      MailSendState.failed => mailSendFailureText(
        status.failure ?? unknownSendFailure,
      ),
      MailSendState.pending => '',
    });
  }

  void _end(String text) {
    _stopTimer();
    _phase = MailSendPhase.finished;
    _notice = text;
    _notify();
  }

  static const String _expiredText =
      'La aprobación caducó sin respuesta. No se ha enviado nada.';
  static const String _deniedText =
      'La rechazaste. No se ha enviado nada. '
      'Cambia el borrador para pedir el envío otra vez.';
  static const String _usedText =
      'La aprobación ya se usó. Revisa Enviados antes de pedir otro envío.';

  /// One host call. A refusal becomes [_notice] and the result is null, so the
  /// screen never sees an exception.
  Future<T?> _run<T>(Future<T> Function() call) async {
    _busy = true;
    _notice = null;
    _notify();
    try {
      return await call();
    } on GatewayError catch (error) {
      _notice = mailFailureText(error);
      return null;
    } on PlanRefused catch (error) {
      // A local check in Rust: its Spanish text is the reason, nothing was sent.
      _notice = error.problem?.message ?? mailCodeText(null);
      return null;
    } on FormatException {
      _notice = mailCodeText(null);
      return null;
    } finally {
      _busy = false;
      _notify();
    }
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    _stopTimer();
    super.dispose();
  }
}
