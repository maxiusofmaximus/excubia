part of 'mail_api.dart';

/// `consumers` for personal accounts, `organizations` for work or school ones.
/// The contract's `common` tenant is left out: it fails with personal accounts.
enum MailTenant {
  consumers('consumers'),
  organizations('organizations');

  const MailTenant(this.wire);

  final String wire;
}

/// A connected mailbox as the host lists it. The host sends no tokens and this
/// model has no field for them.
class MailAccount {
  const MailAccount({
    required this.label,
    required this.tenant,
    required this.upn,
    required this.sendEnabled,
  });

  factory MailAccount.fromJson(Map<String, Object?> json) => MailAccount(
    label: jsonText(json, 'label'),
    tenant: jsonText(json, 'tenant'),
    upn: jsonOptionalText(json, 'upn'),
    sendEnabled: json['send_enabled'] == true,
  );

  final String label;
  final String tenant;
  final String? upn;

  /// True only when the account holds the send consent (`Mail.Send`).
  final bool sendEnabled;
}

/// A draft as the operator reads it before sending. Only the operator's route
/// returns the body; the agent's `mail.draft_get` never does.
class MailDraft {
  const MailDraft({
    required this.draftId,
    required this.from,
    required this.to,
    required this.cc,
    required this.subject,
    required this.body,
    required this.attachments,
  });

  factory MailDraft.fromJson(Map<String, Object?> json) => MailDraft(
    draftId: jsonText(json, 'draft_id'),
    from: jsonOptionalText(json, 'from'),
    to: jsonStrings(json, 'to'),
    cc: jsonStrings(json, 'cc'),
    subject: jsonOptionalText(json, 'subject') ?? '',
    body: jsonOptionalText(json, 'body') ?? '',
    attachments: [
      for (final item in jsonList(json, 'attachments'))
        MailAttachment.fromJson(jsonObject(item)),
    ],
  );

  final String draftId;
  final String? from;
  final List<String> to;
  final List<String> cc;
  final String subject;
  final String body;
  final List<MailAttachment> attachments;
}

/// One attachment as the host digests it: the name, the size and the SHA-256 of
/// the file the approval covers. The content never travels.
class MailAttachment {
  const MailAttachment({
    required this.name,
    required this.size,
    required this.sha256,
  });

  factory MailAttachment.fromJson(Map<String, Object?> json) => MailAttachment(
    name: jsonText(json, 'name'),
    size: jsonInt(json, 'size'),
    sha256: jsonText(json, 'sha256'),
  );

  final String name;
  final int size;
  final String sha256;
}

/// The first answer of a send: nothing leaves yet. The operator approves the
/// `mail.send` in the chat, and only then the app confirms with [approvalId].
class MailSendRequest {
  const MailSendRequest({required this.approvalId, required this.expiresIn});

  final String approvalId;

  /// Seconds the approval stays open (120 on the host).
  final int expiresIn;
}

/// The second answer: the send is scheduled and can still be cancelled for
/// [windowSeconds].
class MailSendWindow {
  const MailSendWindow({required this.sendId, required this.windowSeconds});

  final String sendId;
  final int windowSeconds;
}

enum MailSendState {
  pending,
  sent,
  cancelled,
  failed;

  static MailSendState fromWire(String value) => switch (value) {
    'pending' => pending,
    'sent' => sent,
    'cancelled' => cancelled,
    'failed' => failed,
    _ => throw const FormatException('estado de envío desconocido'),
  };

  /// A state that will not change again.
  bool get isFinal => this != pending;
}

/// Why a send failed, as the host codes it (contract `mail.md`). A code the app
/// does not know is [unknown].
enum MailFailureCode {
  accountSuspended,
  authExpired,
  throttled,
  graphUnavailable,
  badRequest,
  draftChanged,
  interrupted,
  unknown;

  static MailFailureCode fromWire(String? value) => switch (value) {
    'account_suspended' => accountSuspended,
    'auth_expired' => authExpired,
    'throttled' => throttled,
    'graph_unavailable' => graphUnavailable,
    'bad_request' => badRequest,
    'draft_changed' => draftChanged,
    'interrupted' => interrupted,
    _ => unknown,
  };
}

/// [safe] is true only when the host says the message did not leave
/// (`retry: safe`). Otherwise it may have left.
class MailSendFailure {
  const MailSendFailure(this.code, {required this.safe});

  final MailFailureCode code;
  final bool safe;
}

/// The failure an old host or an unknown code gets: the message may have left.
const MailSendFailure unknownSendFailure = MailSendFailure(
  MailFailureCode.unknown,
  safe: false,
);

/// A send as the host reports it. [failure] is set on `failed` and nowhere else.
class MailSendStatus {
  const MailSendStatus(this.state, {this.failure});

  final MailSendState state;
  final MailSendFailure? failure;
}

/// The code and the page the operator needs. The device code stays on the host.
class MailLoginStart {
  const MailLoginStart({
    required this.userCode,
    required this.verificationUri,
    required this.expiresIn,
  });

  factory MailLoginStart.fromJson(Map<String, Object?> json) => MailLoginStart(
    userCode: jsonText(json, 'user_code'),
    verificationUri: jsonText(json, 'verification_uri'),
    expiresIn: jsonInt(json, 'expires_in'),
  );

  final String userCode;
  final String verificationUri;

  /// Seconds from the moment the host issued the code.
  final int expiresIn;
}

enum MailLoginOutcome { pending, ok, denied, expired, error }

/// What the host last said about a login. [upn] is set on `ok`; [code] is a
/// stable host code on `error` and is never shown as is.
class MailLoginStatus {
  const MailLoginStatus(this.outcome, {this.upn, this.code});

  final MailLoginOutcome outcome;
  final String? upn;
  final String? code;
}
