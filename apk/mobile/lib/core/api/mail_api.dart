import 'dart:convert';

import '../../src/rust/api/approvals/model.dart';
import '../../src/rust/api/mail.dart';
import 'approval_find.dart';
import 'chat_transport.dart';
import 'connection_text.dart';
import 'host_client.dart';
import 'json_read.dart';
import 'plan_runner.dart';

part 'mail_failure_text.dart';
part 'mail_models.dart';

/// The mail routes of `/v1/mail` (contract `mail.md`). Every route needs the
/// admin permission; a device without it gets `403` on all of them.
class MailApi {
  const MailApi(this._transport);

  final ChatTransport _transport;

  Future<List<MailAccount>> accounts() async {
    final body = await _read('/v1/mail/accounts');
    return [
      for (final item in jsonList(body, 'accounts'))
        MailAccount.fromJson(jsonObject(item)),
    ];
  }

  Future<MailLoginStart> startLogin({
    required String account,
    required MailTenant tenant,
  }) async {
    final body = await _write('/v1/mail/login/start', {
      'account': account,
      'tenant': tenant.wire,
    });
    return MailLoginStart.fromJson(body);
  }

  Future<MailLoginStatus> loginStatus(String account) async {
    final body = await _read(_statusPath('/v1/mail/login/status', account));
    return _statusFrom(body);
  }

  /// Opens the second consent, the one that grants `Mail.Send`. It is never
  /// part of a normal login.
  Future<MailLoginStart> startSendConsent(String account) async {
    final body = await _write('/v1/mail/send/enable', {'account': account});
    return MailLoginStart.fromJson(body);
  }

  Future<MailLoginStatus> sendConsentStatus(String account) async {
    final body = await _read(
      _statusPath('/v1/mail/send/enable/status', account),
    );
    return _statusFrom(body);
  }

  Future<void> disableSend(String account) async {
    await _write('/v1/mail/send/disable', {'account': account});
  }

  /// The draft the operator reviews. Needs the id; the host offers no listing.
  Future<MailDraft> draft(String account, String draftId) async {
    final body = await _read(
      '/v1/mail/drafts/${Uri.encodeComponent(draftId)}'
      '?account=${Uri.encodeComponent(account)}',
    );
    return MailDraft.fromJson(jsonObject(body['draft']));
  }

  /// Replaces the draft's content. The host closes an open send on it. Local
  /// checks run in Rust first: a refusal arrives as [PlanRefused], never as a call.
  Future<bool> updateDraft({
    required String account,
    required String draftId,
    required List<String> to,
    required List<String> cc,
    required String subject,
    required String body,
  }) async {
    await runPlan(
      _transport,
      planDraftUpdate(
        account: account,
        draftId: draftId,
        to: to,
        cc: cc,
        subject: subject,
        body: body,
      ),
    );
    return true;
  }

  /// Asks for approval. Nothing is sent by this call.
  Future<MailSendRequest> requestSend(String account, String draftId) async {
    final body = jsonEnvelope(
      await runPlan(
        _transport,
        planDraftSend(account: account, draftId: draftId),
      ),
    );
    _expectState(body, 'awaiting_approval');
    return MailSendRequest(
      approvalId: jsonText(body, 'approval_id'),
      expiresIn: jsonInt(body, 'expires_in'),
    );
  }

  /// Uses an approval the operator gave in the chat. This is the only call that
  /// schedules a send.
  Future<MailSendWindow> confirmSend({
    required String account,
    required String draftId,
    required String approvalId,
  }) async {
    final body = jsonEnvelope(
      await runPlan(
        _transport,
        planDraftConfirm(
          account: account,
          draftId: draftId,
          approvalId: approvalId,
        ),
      ),
    );
    _expectState(body, 'pending');
    return MailSendWindow(
      sendId: jsonText(body, 'send_id'),
      windowSeconds: jsonInt(body, 'window_seconds'),
    );
  }

  Future<MailSendStatus> sendState(String sendId) async {
    final body = await _read('/v1/mail/sends/${Uri.encodeComponent(sendId)}');
    return _sendStatusFrom(body);
  }

  Future<MailSendStatus> cancelSend(String sendId) async {
    final body = await _write(
      '/v1/mail/sends/${Uri.encodeComponent(sendId)}/cancel',
      const {},
    );
    return _sendStatusFrom(body);
  }

  /// The approval as the chat sees it, for the operator's answer, read from the
  /// inbox (see [findApproval]). Null while no list has it. The host sends
  /// `details` only to a device with `APPROVE`, which this app has.
  Future<ChatApproval?> approval(String approvalId) =>
      findApproval(_transport, approvalId);

  static void _expectState(Map<String, Object?> body, String state) {
    if (jsonText(body, 'state') != state) {
      throw const FormatException('respuesta de envío inesperada');
    }
  }

  Future<Map<String, Object?>> _read(String target) async =>
      jsonEnvelope(await _transport.call('GET', target));

  Future<Map<String, Object?>> _write(
    String target,
    Map<String, Object?> json,
  ) async => jsonEnvelope(
    await _transport.call('POST', target, utf8.encode(jsonEncode(json))),
  );

  /// `encodeComponent` writes a space as `%20`, which every parser reads.
  static String _statusPath(String route, String account) =>
      '$route?account=${Uri.encodeComponent(account)}';
}

MailLoginStatus _statusFrom(Map<String, Object?> body) {
  final login = jsonObject(body['login']);
  return switch (jsonText(login, 'status')) {
    'pending' => const MailLoginStatus(MailLoginOutcome.pending),
    'ok' => MailLoginStatus(
      MailLoginOutcome.ok,
      upn: jsonOptionalText(login, 'upn'),
    ),
    'denied' => const MailLoginStatus(MailLoginOutcome.denied),
    'expired' => const MailLoginStatus(MailLoginOutcome.expired),
    'error' => MailLoginStatus(
      MailLoginOutcome.error,
      code: jsonOptionalText(login, 'code'),
    ),
    _ => throw const FormatException('estado de inicio de sesión desconocido'),
  };
}

MailSendStatus _sendStatusFrom(Map<String, Object?> body) {
  final state = MailSendState.fromWire(jsonText(body, 'state'));
  if (state != MailSendState.failed) return MailSendStatus(state);
  // An old host sends no `failure`: an empty map makes it unknown. The server's
  // own text is never read.
  final failure = jsonObjectOrEmpty(body['failure']);
  final code = MailFailureCode.fromWire(jsonOptionalText(failure, 'code'));
  final safe =
      code != MailFailureCode.unknown &&
      jsonOptionalText(failure, 'retry') == 'safe';
  return MailSendStatus(state, failure: MailSendFailure(code, safe: safe));
}
