import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/chat_transport.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/core/api/mail_api.dart';
import 'package:herdr_notify/core/state/mail_draft_controller.dart';
import 'package:herdr_notify/core/state/mail_settings_controller.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';

import 'support/approval_fixtures.dart';

// The hook route: a signed device never reads one approval by id (401).
final RegExp _approvalById = RegExp(
  r'^/v1/chat/approvals/(?!settings$)[^/?]+$',
);

// The device inbox, with or without `?status=`.
final RegExp _inboxRoute = RegExp(r'^/v1/chat/approvals(?:\?status=(\w+))?$');

/// A host that answers by route ("METHOD target"). A route with several answers
/// gives them in order and then repeats the last one; a [GatewayError] answer
/// is thrown the way the real client refuses.
///
/// The approval lives in the inbox, as on the host. [approvals] is its status
/// per step: each pending-inbox read takes the next one, the last repeats, and
/// the resolved-status reads see the status taken last.
class _Host implements ChatTransport {
  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError();

  @override
  Future<List<int>> callBytes(String method, String target) =>
      throw UnimplementedError();

  _Host(this.answers, {List<String> approvals = const ['pending']})
    : _steps = List.of(approvals);

  final Map<String, List<Object>> answers;
  final List<String> calls = [];

  /// Every call with its body, so a test can see what was sent and when.
  final List<String> sent = [];

  final List<String> _steps;
  String _status = 'pending';

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    final key = '$method $target';
    calls.add(key);
    final text = utf8.decode(body);
    if (method == 'POST' && text.isNotEmpty) sent.add('$key $text');
    if (method == 'GET' && _approvalById.hasMatch(target)) {
      throw const GatewayError(401, '{"detail":"unauthorized"}');
    }
    final inbox = _inboxRoute.firstMatch(target);
    if (method == 'GET' && inbox != null) return _readInbox(inbox.group(1));
    final queue = answers[key] ?? const <Object>[];
    if (queue.isEmpty) throw const GatewayError(404, '{"detail":"not found"}');
    final answer = queue.length > 1 ? queue.removeAt(0) : queue.first;
    if (answer is GatewayError) throw answer;
    return answer as String;
  }

  String _readInbox(String? status) {
    if (status == null) {
      _status = _steps.length > 1 ? _steps.removeAt(0) : _steps.first;
    }
    final listed = _status == (status ?? 'pending');
    return inboxJson([if (listed) approvalRow(id: 'apr_1', status: _status)]);
  }

  int sendsOf(String key) => sent.where((call) => call.startsWith(key)).length;
}

String _ok(Map<String, Object?> fields) => jsonEncode({'ok': true, ...fields});

const String _draftRoute = 'GET /v1/mail/drafts/dft_1?account=trabajo';
const String _sendRoute = 'POST /v1/mail/drafts/dft_1/send';
const String _statusRoute = 'GET /v1/mail/sends/snd_1';
const String _cancelRoute = 'POST /v1/mail/sends/snd_1/cancel';

final String _draft = _ok({
  'draft': {
    'account': 'trabajo',
    'draft_id': 'dft_1',
    'from': 'ana@ejemplo.com',
    'to': ['bea@ejemplo.org'],
    'cc': <String>[],
    'subject': 'Presupuesto',
    'body': 'Hola Bea',
    'content_type': 'text',
    'attachments': [
      {'name': 'presupuesto.pdf', 'size': 12034, 'sha256': 'abcdef0123456789'},
    ],
  },
});

final String _awaiting = _ok({
  'state': 'awaiting_approval',
  'approval_id': 'apr_1',
  'expires_in': 120,
});

final String _pending = _ok({
  'state': 'pending',
  'send_id': 'snd_1',
  'window_seconds': 10,
});

Map<String, List<Object>> _routes({
  List<Object> sends = const [],
  List<Object> statuses = const [],
  List<Object> cancels = const [],
}) => {
  _draftRoute: [_draft],
  _sendRoute: sends.isEmpty ? [_awaiting] : sends,
  _statusRoute: statuses.isEmpty
      ? [
          _ok({'state': 'pending'}),
        ]
      : statuses,
  _cancelRoute: cancels.isEmpty
      ? [
          _ok({'send_id': 'snd_1', 'state': 'cancelled'}),
        ]
      : cancels,
};

MailDraftController _controller(_Host host) => MailDraftController(
  MailApi(host),
  account: 'trabajo',
  draftId: 'dft_1',
  tick: const Duration(milliseconds: 1),
);

/// Runs one approved send to its end, where the host says `failed` with [failure].
Future<MailDraftController> _failedSend(
  Map<String, Object?> failure, {
  Map<String, List<Object>> extra = const {},
}) async {
  final host = _Host(
    {
      ..._routes(
        sends: [
          _awaiting,
          _ok({'state': 'pending', 'send_id': 'snd_1', 'window_seconds': 0}),
        ],
        statuses: [
          _ok({'state': 'failed', 'failure': failure}),
        ],
      ),
      ...extra,
    },
    approvals: ['approved'],
  );
  final controller = _controller(host);
  await controller.load();
  await controller.requestSend();
  await _until(() => controller.phase == MailSendPhase.finished);
  return controller;
}

/// Lets the controller's timer run until [done] holds, or fails the test.
Future<void> _until(bool Function() done) async {
  for (var i = 0; i < 1000 && !done(); i++) {
    await Future<void>.delayed(const Duration(milliseconds: 2));
  }
  expect(done(), isTrue);
}

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  test(
    'loads the draft and asks for approval without sending anything',
    () async {
      final host = _Host(_routes());
      final controller = _controller(host);
      addTearDown(controller.dispose);

      await controller.load();
      expect(controller.draft?.subject, 'Presupuesto');
      await controller.requestSend();
      await Future<void>.delayed(const Duration(milliseconds: 30));

      expect(controller.phase, MailSendPhase.awaitingApproval);
      expect(host.sendsOf(_sendRoute), 1);
      expect(host.sent.single, isNot(contains('approval_id')));
    },
  );

  test('a pending approval never schedules the send on its own', () async {
    final host = _Host(_routes(), approvals: ['pending']);
    final controller = _controller(host);
    addTearDown(controller.dispose);

    await controller.load();
    await controller.requestSend();
    await Future<void>.delayed(const Duration(milliseconds: 60));

    expect(controller.phase, MailSendPhase.awaitingApproval);
    expect(host.sendsOf(_sendRoute), 1);
  });

  test('the device never reads one approval by id', () async {
    final host = _Host(
      _routes(sends: [_awaiting, _pending]),
      approvals: ['approved'],
    );
    final controller = _controller(host);
    addTearDown(controller.dispose);

    await controller.load();
    await controller.requestSend();
    await _until(() => controller.phase == MailSendPhase.scheduled);

    expect(
      host.calls.where((c) => c.contains('/v1/chat/approvals/apr_1')),
      isEmpty,
    );
    expect(host.calls, contains('GET /v1/chat/approvals'));
    expect(host.calls, contains('GET /v1/chat/approvals?status=approved'));
  });

  test('a denied approval ends the flow and nothing is sent', () async {
    final host = _Host(_routes(), approvals: ['pending', 'denied']);
    final controller = _controller(host);
    addTearDown(controller.dispose);

    await controller.load();
    await controller.requestSend();
    await _until(() => controller.phase == MailSendPhase.finished);

    expect(
      controller.notice,
      'La rechazaste. No se ha enviado nada. Cambia el borrador para pedir el envío otra vez.',
    );
    expect(host.sendsOf(_sendRoute), 1);
    // The host refuses the same content until the draft changes, so no new send.
    expect(controller.canRequestSend, isFalse);
    expect(controller.canEdit, isTrue);
  });

  test('after a denial, an edit lets the draft be asked again', () async {
    final host = _Host(
      {
        ..._routes(),
        'POST /v1/mail/drafts/dft_1/update': [_ok({})],
      },
      approvals: ['denied'],
    );
    final controller = _controller(host);
    addTearDown(controller.dispose);

    await controller.load();
    await controller.requestSend();
    await _until(() => controller.finished);
    final saved = await controller.update(
      to: const ['bea@ejemplo.org'],
      cc: const [],
      subject: 'Presupuesto',
      body: 'Hola Bea, versión 2',
    );

    expect(saved, isTrue);
    expect(controller.canRequestSend, isTrue);
  });

  test(
    'an expired approval is asked again for the same draft, with no edit',
    () async {
      final host = _Host(
        _routes(sends: [_awaiting, _awaiting, _pending]),
        approvals: ['expired', 'approved'],
      );
      final controller = _controller(host);
      addTearDown(controller.dispose);

      await controller.load();
      await controller.requestSend();
      await _until(() => controller.phase == MailSendPhase.finished);
      expect(controller.notice, contains('caducó'));

      await controller.requestSend();
      await _until(() => controller.phase == MailSendPhase.scheduled);

      expect(host.sendsOf(_sendRoute), 3);
      expect(host.sent.last, contains('"approval_id":"apr_1"'));
    },
  );

  test('a failed send says so in Spanish and offers no new send', () async {
    final host = _Host(
      _routes(
        sends: [
          _awaiting,
          _ok({'state': 'pending', 'send_id': 'snd_1', 'window_seconds': 0}),
        ],
        statuses: [
          _ok({'state': 'failed'}),
        ],
      ),
      approvals: ['approved'],
    );
    final controller = _controller(host);
    addTearDown(controller.dispose);

    await controller.load();
    await controller.requestSend();
    await _until(() => controller.phase == MailSendPhase.finished);

    // Graph may have accepted the mail, so a second send is not offered.
    expect(
      controller.notice,
      'El envío no se completó y puede que el correo haya salido. Revisa Enviados antes de volver a intentarlo.',
    );
    expect(controller.canRequestSend, isFalse);
    expect(controller.canEdit, isFalse);
  });

  test(
    'a failure that says nothing left allows an edit, never a new send',
    () async {
      final controller = await _failedSend(
        {'code': 'graph_unavailable', 'retry': 'safe'},
        extra: {
          'POST /v1/mail/drafts/dft_1/update': [_ok({})],
        },
      );
      addTearDown(controller.dispose);

      expect(
        controller.notice,
        'No se pudo contactar con Microsoft. No se ha enviado nada.',
      );
      expect(controller.canRequestSend, isFalse);
      expect(controller.canEdit, isTrue);

      final saved = await controller.update(
        to: const ['bea@ejemplo.org'],
        cc: const [],
        subject: 'Presupuesto',
        body: 'Hola Bea, otra vez',
      );

      expect(saved, isTrue);
      expect(controller.failure, isNull);
      expect(controller.canRequestSend, isTrue);
    },
  );

  test('a failure that may have left the mail allows no edit', () async {
    final controller = await _failedSend({
      'code': 'interrupted',
      'retry': 'check_sent',
    });
    addTearDown(controller.dispose);

    expect(controller.failure?.code, MailFailureCode.interrupted);
    expect(controller.notice, contains('puede que el correo haya salido'));
    expect(controller.canRequestSend, isFalse);
    expect(controller.canEdit, isFalse);
  });

  test(
    'an expired login names the account to reconnect, and no edit',
    () async {
      final controller = await _failedSend({
        'code': 'auth_expired',
        'retry': 'no',
      });
      addTearDown(controller.dispose);

      expect(controller.notice, contains('Vuelve a conectar la cuenta'));
      expect(controller.canEdit, isFalse);
    },
  );

  test('an expired approval says it timed out and sends nothing', () async {
    final host = _Host(_routes(), approvals: ['expired']);
    final controller = _controller(host);
    addTearDown(controller.dispose);

    await controller.load();
    await controller.requestSend();
    await _until(() => controller.phase == MailSendPhase.finished);

    expect(controller.notice, contains('caducó'));
    expect(host.sendsOf(_sendRoute), 1);
  });

  test('an approved send opens the window and cancelling stops it', () async {
    final host = _Host(
      _routes(sends: [_awaiting, _pending]),
      approvals: ['approved'],
    );
    final controller = _controller(host);
    addTearDown(controller.dispose);

    await controller.load();
    await controller.requestSend();
    await _until(() => controller.phase == MailSendPhase.scheduled);
    expect(host.sent.last, contains('"approval_id":"apr_1"'));
    expect(controller.canCancel, isTrue);

    await controller.cancel();

    expect(controller.phase, MailSendPhase.finished);
    expect(controller.notice, 'Envío cancelado. No se ha enviado nada.');
    expect(host.calls, contains(_cancelRoute));
  });

  test('when the window closes the host has the last word: sent', () async {
    final host = _Host(
      _routes(
        sends: [
          _awaiting,
          _ok({'state': 'pending', 'send_id': 'snd_1', 'window_seconds': 0}),
        ],
        statuses: [
          _ok({'state': 'pending'}),
          _ok({'state': 'sent'}),
        ],
      ),
      approvals: ['approved'],
    );
    final controller = _controller(host);
    addTearDown(controller.dispose);

    await controller.load();
    await controller.requestSend();
    await _until(() => controller.phase == MailSendPhase.finished);

    expect(controller.notice, 'Correo enviado.');
    expect(host.calls.where((c) => c == _statusRoute), hasLength(2));
  });

  test('a used approval says it was stale and sends nothing', () async {
    final host = _Host(_routes(), approvals: ['consumed']);
    final controller = _controller(host);
    addTearDown(controller.dispose);

    await controller.load();
    await controller.requestSend();
    await _until(() => controller.phase == MailSendPhase.finished);

    expect(
      controller.notice,
      'La aprobación ya se usó. Revisa Enviados antes de pedir otro envío.',
    );
    expect(host.sendsOf(_sendRoute), 1);
  });

  test('a stale confirmation ends at once and is not retried', () async {
    const stale = GatewayError(
      400,
      '{"detail":{"code":"mail_approval_stale","message":"x"}}',
    );
    final host = _Host(
      _routes(sends: [_awaiting, stale]),
      approvals: ['approved'],
    );
    final controller = _controller(host);
    addTearDown(controller.dispose);

    await controller.load();
    await controller.requestSend();
    await _until(() => controller.phase == MailSendPhase.finished);
    await Future<void>.delayed(const Duration(milliseconds: 30));

    expect(controller.notice, 'El borrador cambió. Revísalo de nuevo.');
    expect(host.sendsOf(_sendRoute), 2);
    expect(controller.canCancel, isFalse);
  });

  test(
    'a rate limit says how long to wait and lets the user try again',
    () async {
      const limited = GatewayError(
        429,
        '{"detail":{"code":"mail_send_rate_limited","message":"x"}}',
        retryAfter: 90,
      );
      final host = _Host(_routes(sends: [limited]));
      final controller = _controller(host);
      addTearDown(controller.dispose);

      await controller.load();
      await controller.requestSend();

      expect(controller.phase, MailSendPhase.idle);
      expect(controller.notice, contains('2 min'));
      expect(controller.canRequestSend, isTrue);
    },
  );

  test(
    'an approval past its 120 seconds expires without a confirmation',
    () async {
      final host = _Host(
        _routes(
          sends: [
            _ok({
              'state': 'awaiting_approval',
              'approval_id': 'apr_1',
              'expires_in': 0,
            }),
          ],
        ),
      );
      final controller = _controller(host);
      addTearDown(controller.dispose);

      await controller.load();
      await controller.requestSend();
      await _until(() => controller.phase == MailSendPhase.finished);

      expect(controller.notice, contains('caducó'));
      expect(host.sendsOf(_sendRoute), 1);
      expect(controller.canRequestSend, isTrue);
    },
  );

  test(
    'when cancel cannot reach the host, the controller asks what happened',
    () async {
      final host = _Host(
        _routes(
          sends: [_awaiting, _pending],
          statuses: [
            _ok({'state': 'cancelled'}),
          ],
          cancels: [const GatewayError(0, 'sin conexión')],
        ),
        approvals: ['approved'],
      );
      final controller = _controller(host);
      addTearDown(controller.dispose);

      await controller.load();
      await controller.requestSend();
      await _until(() => controller.phase == MailSendPhase.scheduled);
      await controller.cancel();
      await _until(() => controller.phase == MailSendPhase.finished);

      expect(controller.notice, 'Envío cancelado. No se ha enviado nada.');
      // Nothing goes out after a cancel, so the draft can be edited and sent again.
      expect(controller.canEdit, isTrue);
      expect(controller.canRequestSend, isTrue);
    },
  );

  test('a draft the host refuses to show ends with a Spanish reason', () async {
    final host = _Host({
      _draftRoute: [
        const GatewayError(
          400,
          '{"detail":{"code":"mail_unknown_draft","message":"x"}}',
        ),
      ],
    });
    final controller = _controller(host);
    addTearDown(controller.dispose);

    await controller.load();

    expect(controller.draft, isNull);
    expect(controller.loadFailure, contains('no lo creó este equipo'));
    expect(controller.loadFailure, isNot(contains('mail_')));
  });

  test('a send under way keeps being watched after its screen is closed', () async {
    final host = _Host(
      _routes(sends: [_awaiting, _pending]),
      approvals: ['approved'],
    );
    final settings = MailSettingsController(
      MailApi(host),
      draftTick: const Duration(milliseconds: 1),
    );
    addTearDown(settings.dispose);

    // The screen that opened the draft is gone: only the settings hold it now.
    final controller = settings.draftFor('trabajo', 'dft_1');
    await controller.load();
    await controller.requestSend();

    await _until(() => controller.phase == MailSendPhase.scheduled);
    expect(host.sent.last, contains('"approval_id":"apr_1"'));
    expect(settings.draftFor('trabajo', 'dft_1'), same(controller));
  });

  test(
    'closing the screen keeps the send, and its result shows on return',
    () async {
      final host = _Host(
        _routes(
          sends: [
            _awaiting,
            _ok({'state': 'pending', 'send_id': 'snd_1', 'window_seconds': 0}),
          ],
          statuses: [
            _ok({'state': 'sent'}),
          ],
        ),
        approvals: ['approved'],
      );
      final settings = MailSettingsController(
        MailApi(host),
        draftTick: const Duration(milliseconds: 1),
      );
      addTearDown(settings.dispose);

      // The sheet is gone while the window runs; the result waits for the next open.
      final first = settings.draftFor('trabajo', 'dft_1');
      await first.load();
      await first.requestSend();
      await _until(() => first.finished);

      final reopened = settings.draftFor('trabajo', 'dft_1');
      expect(reopened, same(first));
      expect(reopened.notice, 'Correo enviado.');
      expect(reopened.canRequestSend, isFalse);
    },
  );

  test('a finished flow stops reading the host', () async {
    final host = _Host(_routes(), approvals: ['denied']);
    final controller = _controller(host);
    addTearDown(controller.dispose);

    await controller.load();
    await controller.requestSend();
    await _until(() => controller.finished);
    final reads = host.calls.length;
    await Future<void>.delayed(const Duration(milliseconds: 30));

    expect(host.calls, hasLength(reads));
  });

  test('a saved edit is sent, read again and needs a new approval', () async {
    const updateRoute = 'POST /v1/mail/drafts/dft_1/update';
    final host = _Host({
      ..._routes(),
      updateRoute: [_ok({})],
    });
    final controller = _controller(host);
    addTearDown(controller.dispose);

    await controller.load();
    final saved = await controller.update(
      to: ['bea@ejemplo.org'],
      cc: const [],
      subject: 'Nuevo asunto',
      body: 'Hola otra vez',
    );

    final sent = host.sent.firstWhere((call) => call.startsWith(updateRoute));
    final body = jsonDecode(sent.substring(updateRoute.length + 1)) as Map;
    expect(saved, isTrue);
    expect(body['subject'], 'Nuevo asunto');
    expect(controller.phase, MailSendPhase.idle);
    expect(controller.notice, contains('hay que aprobar el envío de nuevo'));
  });

  test('an edit the app refuses is not sent and says why', () async {
    final host = _Host(_routes());
    final controller = _controller(host);
    addTearDown(controller.dispose);

    await controller.load();
    final saved = await controller.update(
      to: ['bea@ejemplo.org'],
      cc: const [],
      subject: 'a' * 999,
      body: 'Hola Bea',
    );

    expect(saved, isFalse);
    expect(controller.notice, isNotNull);
    expect(host.calls.where((call) => call.contains('/update')), isEmpty);
  });

  test('the draft can be edited only while no send is open', () async {
    final host = _Host(_routes());
    final controller = _controller(host);
    addTearDown(controller.dispose);

    await controller.load();
    expect(controller.canEdit, isTrue);
    expect(controller.approvalId, isNull);

    await controller.requestSend();

    expect(controller.approvalId, 'apr_1');
    expect(controller.canEdit, isFalse);
  });
}
