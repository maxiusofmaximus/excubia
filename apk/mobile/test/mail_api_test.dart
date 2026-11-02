import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/chat_transport.dart';
import 'package:herdr_notify/core/api/connection_failure.dart';
import 'package:herdr_notify/core/api/connection_text.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/api/mail_api.dart';

/// Answers each call in order and records what was asked. An answer that is a
/// [HostError] is thrown, the way the real client refuses.
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
  group('MailApi routes', () {
    test(
      'lists accounts with GET and keeps only the label, tenant and address',
      () async {
        final transport = _FakeTransport([
          _ok({
            'accounts': [
              {
                'label': 'trabajo',
                'tenant': 'organizations',
                'upn': 'ana@ejemplo.com',
                'access_token': 'NO-DEBE-VERSE',
              },
            ],
          }),
        ]);

        final accounts = await MailApi(transport).accounts();

        expect(transport.calls.single.method, 'GET');
        expect(transport.calls.single.target, '/v1/mail/accounts');
        expect(accounts.single.label, 'trabajo');
        expect(accounts.single.tenant, 'organizations');
        expect(accounts.single.upn, 'ana@ejemplo.com');
      },
    );

    test(
      'starts a login with the tenant and returns the code and page',
      () async {
        final transport = _FakeTransport([
          _ok({
            'login_id': 'L1',
            'user_code': 'ABCD-1234',
            'verification_uri': 'https://microsoft.com/devicelogin',
            'expires_in': 900,
            'interval': 5,
          }),
        ]);

        final start = await MailApi(transport)
            .startLogin(account: 'trabajo', tenant: MailTenant.organizations);

        expect(transport.calls.single.method, 'POST');
        expect(transport.calls.single.target, '/v1/mail/login/start');
        expect(jsonDecode(transport.calls.single.body), {
          'account': 'trabajo',
          'tenant': 'organizations',
        });
        expect(start.userCode, 'ABCD-1234');
        expect(start.verificationUri, 'https://microsoft.com/devicelogin');
        expect(start.expiresIn, 900);
      },
    );

    test('encodes the account in the status query', () async {
      final transport = _FakeTransport([
        _ok({
          'login': {'status': 'pending'},
        }),
      ]);

      await MailApi(transport).loginStatus('mi cuenta&x');

      expect(
        transport.calls.single.target,
        '/v1/mail/login/status?account=mi%20cuenta%26x',
      );
    });

    test('maps every login outcome the host can report', () async {
      final cases = <Map<String, Object?>, MailLoginOutcome>{
        {'status': 'pending'}: MailLoginOutcome.pending,
        {'status': 'denied'}: MailLoginOutcome.denied,
        {'status': 'expired'}: MailLoginOutcome.expired,
        {'status': 'error', 'code': 'mail_auth_failed'}: MailLoginOutcome.error,
        {'status': 'ok', 'label': 'trabajo', 'upn': 'ana@ejemplo.com'}:
            MailLoginOutcome.ok,
      };
      for (final entry in cases.entries) {
        final transport = _FakeTransport([
          _ok({'login': entry.key}),
        ]);
        final status = await MailApi(transport).loginStatus('trabajo');
        expect(status.outcome, entry.value, reason: '${entry.key}');
      }
    });

    test('keeps the upn on ok and the host code on error only', () async {
      final ok = await MailApi(
        _FakeTransport([
          _ok({
            'login': {'status': 'ok', 'upn': 'ana@ejemplo.com'},
          }),
        ]),
      ).loginStatus('trabajo');
      final failed = await MailApi(
        _FakeTransport([
          _ok({
            'login': {'status': 'error', 'code': 'mail_auth_failed'},
          }),
        ]),
      ).loginStatus('trabajo');

      expect(ok.upn, 'ana@ejemplo.com');
      expect(ok.code, isNull);
      expect(failed.upn, isNull);
      expect(failed.code, 'mail_auth_failed');
    });

    test('the send consent is a separate route from the login', () async {
      final transport = _FakeTransport([
        _ok({
          'user_code': 'WXYZ-9999',
          'verification_uri': 'https://microsoft.com/devicelogin',
          'expires_in': 900,
        }),
        _ok({
          'login': {'status': 'pending'},
        }),
        _ok({'account': 'trabajo', 'send_enabled': false}),
      ]);
      final api = MailApi(transport);

      final start = await api.startSendConsent('trabajo');
      await api.sendConsentStatus('trabajo');
      await api.disableSend('trabajo');

      expect(start.userCode, 'WXYZ-9999');
      expect(transport.calls.map((call) => '${call.method} ${call.target}'), [
        'POST /v1/mail/send/enable',
        'GET /v1/mail/send/enable/status?account=trabajo',
        'POST /v1/mail/send/disable',
      ]);
      expect(jsonDecode(transport.calls[0].body), {'account': 'trabajo'});
      expect(jsonDecode(transport.calls[2].body), {'account': 'trabajo'});
    });

    test('a response without ok is refused, not read', () async {
      final api = MailApi(
        _FakeTransport([
          jsonEncode({'ok': false}),
        ]),
      );

      await expectLater(api.accounts(), throwsFormatException);
    });

    test('an unknown login status is refused', () async {
      final api = MailApi(
        _FakeTransport([
          _ok({
            'login': {'status': 'quizas'},
          }),
        ]),
      );

      await expectLater(api.loginStatus('trabajo'), throwsFormatException);
    });

    test('a refusal from the host reaches the caller as a HostError', () async {
      final api = MailApi(
        _FakeTransport([const HostError(403, '{"detail":"forbidden"}')]),
      );

      await expectLater(api.accounts(), throwsA(isA<HostError>()));
    });
  });

  group('mailFailureText', () {
    String textOf(int status, {String body = '', int? retryAfter}) =>
        mailFailureText(HostError(status, body, retryAfter: retryAfter));

    test('no connection says so in plain words, with no raw error', () {
      expect(
        textOf(0, body: 'SocketException'),
        'No se pudo contactar con el equipo.',
      );
    });

    test('a PC that cannot be reached says why, not the generic line', () {
      expect(
        mailFailureText(
          const HostError.transport(ConnectionFailure.noRoute, 'refused'),
        ),
        connectionText(ConnectionFailure.noRoute),
      );
    });

    test('a device without the admin permission is told what to ask for', () {
      expect(textOf(403, body: '{"detail":"forbidden"}'), contains('permiso'));
    });

    test('not configured is told apart from no permission, even on a 403', () {
      final text = textOf(
        403,
        body: '{"detail":{"code":"mail_not_configured"}}',
      );
      expect(text, contains('aún no está configurado'));
      expect(text, contains('quien administra el equipo'));
      expect(text, isNot(contains('permiso')));
    });

    test(
      'every mail code has its own Spanish text, with no raw code shown',
      () {
        const codes = [
          'mail_not_configured',
          'mail_invalid_input',
          'mail_unavailable',
          'mail_auth_failed',
          'mail_auth_required',
          'mail_login_expired',
          'mail_login_denied',
          'mail_admin_consent_required',
          'mail_app_not_registered',
          'mail_send_not_enabled',
          'mail_approval_stale',
          'mail_approval_denied',
        ];
        final generic = mailCodeText('mail_codigo_inventado');
        for (final code in codes) {
          final text = mailCodeText(code);
          expect(text, isNot(generic), reason: code);
          expect(text, isNot(contains('mail_')), reason: code);
        }
      },
    );

    test('the approval and the send codes read as the spec asks', () {
      expect(mailCodeText('mail_approval_stale'), contains('Revísalo'));
      expect(
        mailCodeText('mail_send_not_enabled'),
        contains('Permitir enviar'),
      );
      expect(mailCodeText('mail_approval_denied'), contains('no se aprobó'));
    });

    test('a rate limit says how long to wait, from Retry-After', () {
      final text = textOf(
        429,
        body: '{"code":"mail_send_rate_limited"}',
        retryAfter: 90,
      );
      expect(text, contains('2 min'));
      expect(text, isNot(contains('mail_')));
      expect(
        textOf(429, body: '{"code":"mail_send_rate_limited"}', retryAfter: 30),
        contains('30 s'),
      );
      expect(
        textOf(429, body: '{"code":"mail_send_rate_limited"}'),
        contains('un rato'),
      );
    });

    test('a 429 without a code is still a rate limit', () {
      expect(textOf(429, retryAfter: 60), contains('1 min'));
    });

    test(
      'an unknown code or a body that is not JSON gets the generic text',
      () {
        final generic = mailCodeText(null);
        expect(textOf(400, body: '{"code":"otro"}'), generic);
        expect(textOf(500, body: '<html>'), generic);
        expect(generic, isNot(contains('mail_')));
      },
    );
  });

  group('mailSendFailureText', () {
    String textOf(MailFailureCode code, {bool safe = false}) =>
        mailSendFailureText(MailSendFailure(code, safe: safe));

    test('an account the host suspended says so and says what to do', () {
      expect(
        textOf(MailFailureCode.accountSuspended),
        'Microsoft suspendió temporalmente el envío de esta cuenta. Entra en Outlook.com para verificarla y vuelve a intentarlo más tarde.',
      );
    });

    test('an expired session points to Ajustes › Correo', () {
      expect(
        textOf(MailFailureCode.authExpired, safe: true),
        'La sesión de la cuenta de correo caducó. Vuelve a conectar la cuenta en Ajustes › Correo.',
      );
    });

    test('a throttle says nothing was sent', () {
      expect(
        textOf(MailFailureCode.throttled, safe: true),
        'Microsoft pidió esperar antes de enviar más correos. No se ha enviado nada; inténtalo en unos minutos.',
      );
    });

    test(
      'an unreachable Microsoft that sent nothing says nothing was sent',
      () {
        expect(
          textOf(MailFailureCode.graphUnavailable, safe: true),
          'No se pudo contactar con Microsoft. No se ha enviado nada.',
        );
      },
    );

    test('a bad request asks to review recipients and subject', () {
      expect(
        textOf(MailFailureCode.badRequest, safe: true),
        'Microsoft rechazó el borrador. Revisa destinatarios y asunto.',
      );
    });

    test('a draft changed after approval says nothing was sent', () {
      expect(
        textOf(MailFailureCode.draftChanged, safe: true),
        'El borrador cambió después de aprobarlo. No se ha enviado nada; pide el envío otra vez.',
      );
    });

    test('an unreachable Microsoft that may have sent says to check Enviados', () {
      const uncertain =
          'El envío no se completó y puede que el correo haya salido. Revisa Enviados antes de volver a intentarlo.';
      expect(textOf(MailFailureCode.graphUnavailable), uncertain);
      expect(textOf(MailFailureCode.interrupted), uncertain);
      expect(textOf(MailFailureCode.unknown), uncertain);
    });

    test('no host code shows through any text', () {
      for (final code in MailFailureCode.values) {
        expect(textOf(code), isNot(contains('mail_')), reason: code.name);
        expect(
          textOf(code, safe: true),
          isNot(contains('_')),
          reason: code.name,
        );
      }
    });
  });
}
