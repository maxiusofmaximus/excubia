import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/chat_transport.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/mail_api.dart';
import 'package:excubia/src/rust/frb_generated.dart';

/// Answers each call in order and records what was asked, as in mail_api_test.
class _FakeTransport implements ChatTransport {
  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError();

  @override
  Future<List<int>> callBytes(String method, String target) =>
      throw UnimplementedError();

  _FakeTransport(this._answers);

  final List<Object> _answers;
  final List<({String method, String target, String body})> calls = [];

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    calls.add((method: method, target: target, body: utf8.decode(body)));
    final answer = _answers.removeAt(0);
    if (answer is HostError) throw answer;
    return answer as String;
  }
}

String _ok(Map<String, Object?> fields) => jsonEncode({'ok': true, ...fields});

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  test(
    'an account says whether it may send, from the host flag only',
    () async {
      final transport = _FakeTransport([
        _ok({
          'accounts': [
            {
              'label': 'trabajo',
              'tenant': 'organizations',
              'send_enabled': true,
            },
            {'label': 'personal', 'tenant': 'consumers'},
          ],
        }),
      ]);

      final accounts = await MailApi(transport).accounts();

      expect(accounts[0].sendEnabled, isTrue);
      expect(accounts[1].sendEnabled, isFalse);
    },
  );

  test(
    'a draft is read with the account and returns the body to the operator',
    () async {
      final transport = _FakeTransport([
        _ok({
          'draft': {
            'account': 'trabajo',
            'draft_id': 'dft 1',
            'from': 'ana@ejemplo.com',
            'to': ['bea@ejemplo.org'],
            'cc': ['carlos@ejemplo.org'],
            'subject': 'Presupuesto',
            'body': 'Hola Bea',
            'content_type': 'text',
            'attachments': [
              {'name': 'presupuesto.pdf', 'size': 12034, 'sha256': 'ab12'},
            ],
          },
        }),
      ]);

      final draft = await MailApi(transport).draft('trabajo', 'dft 1');

      expect(transport.calls.single.method, 'GET');
      expect(
        transport.calls.single.target,
        '/v1/mail/drafts/dft%201?account=trabajo',
      );
      expect(draft.body, 'Hola Bea');
      expect(draft.cc, ['carlos@ejemplo.org']);
      expect(draft.attachments.single.size, 12034);
    },
  );

  test(
    'asking for a send sends the account only and returns the approval',
    () async {
      final transport = _FakeTransport([
        _ok({
          'state': 'awaiting_approval',
          'approval_id': 'apr_1',
          'expires_in': 120,
        }),
      ]);

      final request = await MailApi(transport).requestSend('trabajo', 'dft_1');

      expect(transport.calls.single.target, '/v1/mail/drafts/dft_1/send');
      expect(jsonDecode(transport.calls.single.body), {'account': 'trabajo'});
      expect(request.approvalId, 'apr_1');
      expect(request.expiresIn, 120);
    },
  );

  test(
    'a request that did not wait for approval is refused as unexpected',
    () async {
      final transport = _FakeTransport([
        _ok({'state': 'pending', 'send_id': 'snd_1', 'window_seconds': 10}),
      ]);

      await expectLater(
        MailApi(transport).requestSend('trabajo', 'dft_1'),
        throwsFormatException,
      );
    },
  );

  test('confirming sends the approval id and opens the window', () async {
    final transport = _FakeTransport([
      _ok({'state': 'pending', 'send_id': 'snd_1', 'window_seconds': 10}),
    ]);

    final window = await MailApi(transport)
        .confirmSend(account: 'trabajo', draftId: 'dft_1', approvalId: 'apr_1');

    expect(jsonDecode(transport.calls.single.body), {
      'account': 'trabajo',
      'approval_id': 'apr_1',
    });
    expect(window.sendId, 'snd_1');
    expect(window.windowSeconds, 10);
  });

  test(
    'a send state is one of the four the host names, and no other',
    () async {
      final transport = _FakeTransport([
        _ok({'send_id': 'snd_1', 'state': 'sent'}),
        _ok({'send_id': 'snd_1', 'state': 'vaya'}),
      ]);
      final mail = MailApi(transport);

      expect((await mail.sendState('snd_1')).state, MailSendState.sent);
      await expectLater(mail.sendState('snd_1'), throwsFormatException);
      expect(MailSendState.pending.isFinal, isFalse);
      expect(MailSendState.failed.isFinal, isTrue);
    },
  );

  test(
    'cancelling posts to the send and returns the state it reached',
    () async {
      final transport = _FakeTransport([
        _ok({'send_id': 'snd_1', 'state': 'cancelled'}),
      ]);

      final status = await MailApi(transport).cancelSend('snd_1');

      expect(transport.calls.single.method, 'POST');
      expect(transport.calls.single.target, '/v1/mail/sends/snd_1/cancel');
      expect(status.state, MailSendState.cancelled);
    },
  );

  test(
    'an approval read from the inbox keeps the mail details and drops the rest',
    () async {
      final transport = _FakeTransport([
        jsonEncode({
          'ok': true,
          'approvals': [
            {
              'id': 'apr_1',
              'status': 'pending',
              'expires_at': '2026-10-25T10:12:00.000Z',
              'details': {
                'account': 'trabajo',
                'from': 'ana@ejemplo.com',
                'to': ['bea@ejemplo.org'],
                'subject': 'Presupuesto',
                'body_preview': 'Hola Bea',
                'attachments': <Object>[],
                'device_code': 'DEVICE-SECRETO',
              },
            },
          ],
        }),
      ]);

      final approval = await MailApi(transport).approval('apr_1');

      expect(transport.calls.single.target, '/v1/chat/approvals');
      expect(approval?.status, 'pending');
      expect(approval?.mailSend?.subject, 'Presupuesto');
      expect(approval?.mailAttach, isNull);
    },
  );

  test('a refusal reads its code under detail, and the text is Spanish', () {
    const refusal = HostError(
      400,
      '{"detail":{"code":"mail_send_not_enabled","message":"x"}}',
    );

    final text = mailFailureText(refusal);

    expect(text, contains('Permitir enviar'));
    expect(text, isNot(contains('mail_')));
  });

  test('a rate limit explains the wait from Retry-After', () {
    const limited = HostError(
      429,
      '{"detail":{"code":"mail_send_rate_limited","message":"x"}}',
      retryAfter: 600,
    );

    expect(mailFailureText(limited), contains('10 min'));
  });

  test(
    'a failed send names the host code, and safe only when it did not leave',
    () async {
      final transport = _FakeTransport([
        _failed({'code': 'account_suspended', 'retry': 'no'}),
        _failed({'code': 'graph_unavailable', 'retry': 'safe'}),
        _failed({'code': 'graph_unavailable', 'retry': 'check_sent'}),
        _failed({'code': 'draft_changed', 'retry': 'safe'}),
      ]);
      final mail = MailApi(transport);

      final suspended = (await mail.sendState('snd_1')).failure!;
      final unavailable = (await mail.sendState('snd_1')).failure!;
      final maybeSent = (await mail.sendState('snd_1')).failure!;
      final changed = (await mail.sendState('snd_1')).failure!;

      expect(suspended.code, MailFailureCode.accountSuspended);
      expect(suspended.safe, isFalse);
      expect(unavailable.code, MailFailureCode.graphUnavailable);
      expect(unavailable.safe, isTrue);
      expect(maybeSent.safe, isFalse);
      expect(changed.code, MailFailureCode.draftChanged);
      expect(changed.safe, isTrue);
    },
  );

  test(
    'an unknown code is unknown and never safe, even if the host says so',
    () async {
      final transport = _FakeTransport([
        _failed({'code': 'codigo_nuevo', 'retry': 'safe'}),
      ]);

      final failure = (await MailApi(transport).sendState('snd_1')).failure!;

      expect(failure.code, MailFailureCode.unknown);
      expect(failure.safe, isFalse);
    },
  );

  test(
    'an old host without failure is unknown, and a sent send has none',
    () async {
      final transport = _FakeTransport([
        _ok({'send_id': 'snd_1', 'state': 'failed'}),
        _ok({'send_id': 'snd_1', 'state': 'sent'}),
      ]);
      final mail = MailApi(transport);

      final old = await mail.sendState('snd_1');
      final sent = await mail.sendState('snd_1');

      expect(old.failure?.code, MailFailureCode.unknown);
      expect(old.failure?.safe, isFalse);
      expect(sent.failure, isNull);
    },
  );

  test('the host text of a failure never reaches the Spanish text', () async {
    final transport = _FakeTransport([
      _failed({
        'code': 'graph_unavailable',
        'retry': 'safe',
        'message': 'Bearer SECRETO-123 en mail_x',
      }),
    ]);

    final failure = (await MailApi(transport).sendState('snd_1')).failure!;
    final text = mailSendFailureText(failure);

    expect(text, isNot(contains('SECRETO')));
    expect(text, isNot(contains('mail_')));
  });
}

String _failed(Map<String, Object?> failure) =>
    _ok({'send_id': 'snd_1', 'state': 'failed', 'failure': failure});
