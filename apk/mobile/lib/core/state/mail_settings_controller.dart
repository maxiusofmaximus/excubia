import 'package:flutter/foundation.dart';

import '../api/host_client.dart';
import '../api/mail_api.dart';
import 'mail_draft_controller.dart';
import 'mail_drafts.dart';

/// Which flow a code belongs to: a normal login, or the consent that lets the
/// host send mail from the account.
enum MailPurpose { connect, sendConsent }

/// A code the operator types on the verification page. [issuedAt] is when the
/// app received it, so the countdown starts from there.
class MailCodePrompt {
  const MailCodePrompt({
    required this.account,
    required this.purpose,
    required this.start,
    required this.issuedAt,
  });

  final String account;
  final MailPurpose purpose;
  final MailLoginStart start;
  final DateTime issuedAt;

  DateTime get expiresAt => issuedAt.add(Duration(seconds: start.expiresIn));
}

/// The state of the Correo screen. Every action is one tap and one host call:
/// nothing polls, and a refusal becomes a Spanish sentence in [notice].
class MailSettingsController extends ChangeNotifier {
  MailSettingsController(
    this._mail, {
    DateTime Function()? clock,
    this.draftTick = const Duration(seconds: 1),
  }) : _clock = clock ?? DateTime.now;

  final MailApi _mail;
  final DateTime Function() _clock;

  /// How often an open draft's send reads the host.
  final Duration draftTick;

  List<MailAccount>? _accounts;
  String? _loadFailure;
  bool _busy = false;
  MailCodePrompt? _prompt;
  String? _notice;

  /// The drafts opened from this screen. They belong here and not to the draft's
  /// screen: closing that screen mid-send must not stop the watch on the approval.
  late final MailDrafts _drafts = MailDrafts(
    _mail,
    tick: draftTick,
    clock: _clock,
  );

  /// Null until the first load succeeds.
  List<MailAccount>? get accounts => _accounts;

  /// Why the accounts could not be read, for example a device without admin.
  String? get loadFailure => _loadFailure;
  bool get busy => _busy;
  MailCodePrompt? get prompt => _prompt;
  String? get notice => _notice;

  /// The controller of one draft: the open one while its send is under way,
  /// otherwise a new one. The same draft opened again resumes where it was.
  MailDraftController draftFor(String account, String draftId) =>
      _drafts.draftFor(account, draftId);

  @override
  void dispose() {
    _drafts.dispose();
    super.dispose();
  }

  Future<void> load() async {
    final accounts = await _run(_mail.accounts);
    _accounts = accounts;
    if (accounts == null) {
      _loadFailure = _notice;
      _notice = null;
    } else {
      _loadFailure = null;
    }
    notifyListeners();
  }

  Future<void> connect({required String account, required MailTenant tenant}) =>
      _open(
        MailPurpose.connect,
        account,
        () => _mail.startLogin(account: account, tenant: tenant),
      );

  /// Opens the second consent. The grant is never part of a normal login.
  Future<void> startSendConsent(String account) => _open(
    MailPurpose.sendConsent,
    account,
    () => _mail.startSendConsent(account),
  );

  /// Asks the host once whether the open code was used.
  Future<void> checkPrompt() async {
    final prompt = _prompt;
    if (prompt == null) return;
    final status = await _run(
      () => switch (prompt.purpose) {
        MailPurpose.connect => _mail.loginStatus(prompt.account),
        MailPurpose.sendConsent => _mail.sendConsentStatus(prompt.account),
      },
    );
    if (status == null) return;
    if (status.outcome == MailLoginOutcome.pending) {
      _notice = _outcomeText(prompt, status);
      notifyListeners();
      return;
    }
    _prompt = null;
    // Both purposes change what the account list says (a new account, or the
    // send switch), so the list is read again after a success.
    if (status.outcome == MailLoginOutcome.ok) await load();
    _notice = _outcomeText(prompt, status);
    notifyListeners();
  }

  Future<void> revokeSendConsent(String account) async {
    final revoked = await _run(() async {
      await _mail.disableSend(account);
      return true;
    });
    if (revoked != true) return;
    await load();
    _notice = 'Envío revocado para «$account».';
    notifyListeners();
  }

  Future<void> _open(
    MailPurpose purpose,
    String account,
    Future<MailLoginStart> Function() start,
  ) async {
    final issued = await _run(start);
    if (issued == null) return;
    _prompt = MailCodePrompt(
      account: account,
      purpose: purpose,
      start: issued,
      issuedAt: _clock(),
    );
    notifyListeners();
  }

  /// One host call. A refusal becomes [_notice] and the result is null, so the
  /// caller never sees an exception from the screen's point of view.
  Future<T?> _run<T>(Future<T> Function() call) async {
    _busy = true;
    _notice = null;
    notifyListeners();
    try {
      return await call();
    } on HostError catch (error) {
      _notice = mailFailureText(error);
      return null;
    } on FormatException {
      _notice = mailCodeText(null);
      return null;
    } finally {
      _busy = false;
      notifyListeners();
    }
  }

  String _outcomeText(MailCodePrompt prompt, MailLoginStatus status) =>
      switch (status.outcome) {
        MailLoginOutcome.pending =>
          'Todavía no has introducido el código en Microsoft.',
        MailLoginOutcome.ok when prompt.purpose == MailPurpose.connect =>
          'Cuenta conectada: ${status.upn ?? prompt.account}.',
        MailLoginOutcome.ok =>
          'Envío permitido para «${prompt.account}». '
              'Puedes revocarlo cuando quieras.',
        MailLoginOutcome.denied => mailCodeText('mail_login_denied'),
        MailLoginOutcome.expired => mailCodeText('mail_login_expired'),
        MailLoginOutcome.error => mailCodeText(status.code),
      };
}
