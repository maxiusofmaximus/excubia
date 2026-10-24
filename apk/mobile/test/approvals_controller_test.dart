import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/core/state/approvals_controller.dart';
import 'package:herdr_notify/src/rust/api/approvals/error.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';

import 'support/approval_fixtures.dart';
import 'support/scripted_transport.dart';

const _decide = 'POST /v1/chat/approvals/apr_1/decision';
const _probe = 'POST /v1/chat/approvals/apr_grant_probe/decision';
const _write = 'POST /v1/chat/approvals/settings';
const _read = 'GET /v1/chat/approvals/apr_1';
const _settings = 'GET /v1/chat/approvals/settings';

ScriptedTransport _host({String state = 'denied'}) => ScriptedTransport({
  'GET /v1/chat/approvals': inboxJson(),
  _settings: settingsJson(),
  _write: settingsJson(global: 'always_user'),
  _decide: recordJson(approvalRow(status: 'approved', level: 'user')),
  _read: stateJson(state),
});

Future<ApprovalsController> _loaded(ScriptedTransport host) async {
  final approvals = ApprovalsController(transport: host, deviceId: 'dev_1');
  await approvals.refresh();
  return approvals;
}

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  test(
    'the inbox fills the user queue and the view reads its command',
    () async {
      final approvals = await _loaded(_host());
      expect(approvals.userPending().map((a) => a.approvalId), ['apr_1']);
      final view = approvals.viewOf('apr_1')!;
      expect(view.command, 'cargo test');
      expect(view.cwd, 'demo/api');
      expect(approvals.settings!.global, 'delegated');
      approvals.dispose();
    },
  );

  test('approve sends one decision and shows the host answer', () async {
    final host = _host();
    final approvals = await _loaded(host);
    await approvals.decide('apr_1', approve: true);
    expect(approvals.viewOf('apr_1')!.status, 'approved');
    expect(approvals.failureOf('apr_1'), isNull);
    expect(approvals.userPending(), isEmpty);
    final sent = host.bodies[_decide]!.single;
    expect(sent['decision'], 'approve');
    expect(sent['scope'], 'once');
    expect(sent['client_id'], isA<String>());
    approvals.dispose();
  });

  test('deny sends the deny decision', () async {
    final host = _host();
    final approvals = await _loaded(host);
    await approvals.decide('apr_1', approve: false);
    expect(host.bodies[_decide]!.single['decision'], 'deny');
    approvals.dispose();
  });

  test('a 403 keeps the card pending and names the host command', () async {
    final host = _host()
      ..failNext(_decide, const GatewayError(403, forbiddenBody));
    final approvals = await _loaded(host);
    await approvals.decide('apr_1', approve: true);
    final failure = approvals.failureOf('apr_1')!;
    expect(failure.kind, ApprovalFailureKind.needsGrant);
    expect(failure.command, grantCommand);
    expect(approvals.viewOf('apr_1')!.status, 'pending');
    approvals.dispose();
  });

  test('a 409 already decided shows the real state and re-reads it', () async {
    final host = _host(state: 'denied')
      ..failNext(
        _decide,
        const GatewayError(409, '{"detail":"approval_already_decided"}'),
      );
    final approvals = await _loaded(host);
    await approvals.decide('apr_1', approve: true);
    expect(
      approvals.failureOf('apr_1')!.kind,
      ApprovalFailureKind.alreadyDecided,
    );
    expect(approvals.viewOf('apr_1')!.status, 'denied');
    expect(host.calls, contains(_read));
    approvals.dispose();
  });

  test('a 409 expired closes the card as expired', () async {
    final host = _host(state: 'expired')
      ..failNext(
        _decide,
        const GatewayError(409, '{"detail":"approval_expired"}'),
      );
    final approvals = await _loaded(host);
    await approvals.decide('apr_1', approve: true);
    expect(approvals.failureOf('apr_1')!.kind, ApprovalFailureKind.expired);
    expect(approvals.viewOf('apr_1')!.status, 'expired');
    approvals.dispose();
  });

  test('a retry after a network failure reuses the client_id', () async {
    final host = _host()..failNext(_decide, const GatewayError(0, 'sin red'));
    final approvals = await _loaded(host);
    await approvals.decide('apr_1', approve: true);
    expect(approvals.failureOf('apr_1')!.kind, ApprovalFailureKind.other);
    await approvals.decide('apr_1', approve: true);
    expect(approvals.failureOf('apr_1'), isNull);
    final ids = host.bodies[_decide]!.map((b) => b['client_id']).toList();
    expect(ids, hasLength(2));
    expect(ids[1], ids[0]);
    approvals.dispose();
  });

  test('without APPROVE the settings stay read-only and say why', () async {
    final host = _host()
      ..failNext(_settings, const GatewayError(403, forbiddenBody));
    final approvals = ApprovalsController(transport: host, deviceId: 'dev_1');
    await approvals.loadSettings();
    expect(approvals.settingsDenied, isTrue);
    expect(approvals.settingsFailure!.command, grantCommand);
    expect(await approvals.setMode(mode: 'always_user'), isFalse);
    expect(host.calls, isNot(contains(_write)));
    approvals.dispose();
  });

  test(
    'the global switch and a project override post their own scope',
    () async {
      final host = _host();
      final approvals = await _loaded(host);
      expect(await approvals.setMode(mode: 'always_user'), isTrue);
      expect(
        await approvals.setMode(mode: 'delegated', projectId: 'prj_a'),
        isTrue,
      );
      final writes = host.bodies[_write]!;
      expect(writes[0]['mode'], 'always_user');
      expect(writes[0].containsKey('project_id'), isFalse);
      expect(writes[1]['mode'], 'delegated');
      expect(writes[1]['project_id'], 'prj_a');
      expect(writes[0]['client_id'], isA<String>());
      approvals.dispose();
    },
  );

  test('a chat.approval frame moves the card forward and never back', () async {
    final approvals = await _loaded(_host());
    approvals.onChatEvent(
      parseChatEvent(
        json:
            '{"event":"chat.approval","revision":10,"conversation_id":"cnv_1",'
            '"approval_id":"apr_1","status":"approved","level":"user"}',
      )!,
    );
    expect(approvals.viewOf('apr_1')!.status, 'approved');
    approvals.onChatEvent(
      parseChatEvent(
        json:
            '{"event":"chat.approval","revision":11,"conversation_id":"cnv_1",'
            '"approval_id":"apr_1","status":"pending","level":"user"}',
      )!,
    );
    await Future<void>.delayed(Duration.zero);
    expect(approvals.viewOf('apr_1')!.status, 'approved');
    approvals.dispose();
  });

  test('a recheck clears the grant notice only once the grant is there', () async {
    final host = _host()
      ..failNext(_decide, const GatewayError(403, forbiddenBody));
    final approvals = await _loaded(host);
    await approvals.decide('apr_1', approve: true);
    expect(approvals.failureOf('apr_1')!.kind, ApprovalFailureKind.needsGrant);

    host.failNext(_probe, const GatewayError(403, forbiddenBody));
    expect(await approvals.recheckGrant(), isFalse);
    expect(approvals.failureOf('apr_1')!.kind, ApprovalFailureKind.needsGrant);

    host.failNext(_probe, const GatewayError(404, '{"detail":"not found"}'));
    expect(await approvals.recheckGrant(), isTrue);
    expect(approvals.failureOf('apr_1'), isNull);
    expect(approvals.viewOf('apr_1')!.status, 'pending');
    // The probe never approves: an unknown id can only be denied, if at all.
    expect(
      host.bodies[_probe]!.map((sent) => sent['decision']),
      everyElement('deny'),
    );
    approvals.dispose();
  });

  test('a new pending frame shows its inbox card without a restart', () async {
    final routes = <String, String>{
      'GET /v1/chat/approvals': inboxJson([]),
      _settings: settingsJson(),
    };
    final approvals = ApprovalsController(
      transport: ScriptedTransport(routes),
      deviceId: 'dev_1',
    );
    await approvals.refresh();
    expect(approvals.inboxUnassigned(), isEmpty);
    routes['GET /v1/chat/approvals'] = inboxJson([
      approvalRow(conversationId: null),
    ]);
    approvals.onChatEvent(
      parseChatEvent(
        json:
            '{"event":"chat.approval","revision":12,"conversation_id":null,'
            '"approval_id":"apr_1","status":"pending","level":"user"}',
      )!,
    );
    for (var i = 0; i < 5; i++) {
      await Future<void>.delayed(Duration.zero);
    }
    expect(approvals.inboxUnassigned().map((a) => a.approvalId), ['apr_1']);
    approvals.dispose();
  });
}
