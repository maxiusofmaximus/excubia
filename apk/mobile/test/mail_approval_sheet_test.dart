import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/chat_transport.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/core/api/mail_api.dart';
import 'package:herdr_notify/core/state/approvals_controller.dart';
import 'package:herdr_notify/core/state/mail_draft_controller.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/mail/mail_draft_screen.dart';

import 'support/approval_fixtures.dart';
import 'support/widget_mount.dart';

const String _draftPath = 'GET /v1/mail/drafts/dft_1?account=trabajo';
const String _sendPath = 'POST /v1/mail/drafts/dft_1/send';
const String _decisionPath = 'POST /v1/chat/approvals/apr_1/decision';

/// The host that the operator answers: the draft «dft_1», the inbox that lists
/// `apr_1` with its current status and its `details`, and the decision that moves
/// that status. [approvedSubject] is what the approval covers; the draft always
/// says «Presupuesto».
class _ApprovingHost implements ChatTransport {
  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError();

  @override
  Future<List<int>> callBytes(String method, String target) =>
      throw UnimplementedError();

  _ApprovingHost({this.approvedSubject = 'Presupuesto', this.expiresIn = 120});

  final String approvedSubject;
  final int expiresIn;
  String status = 'pending';
  final List<String> calls = [];

  int get sends => calls.where((call) => call == _sendPath).length;

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    final key = '$method $target';
    calls.add(key);
    final text = utf8.decode(body);
    if (key == _draftPath) return okJson({'draft': _draftJson()});
    if (key == _sendPath) {
      // The first send asks for approval; the confirmation carries its id.
      return text.contains('approval_id')
          ? okJson({
              'state': 'pending',
              'send_id': 'snd_1',
              'window_seconds': 10,
            })
          : okJson({
              'state': 'awaiting_approval',
              'approval_id': 'apr_1',
              'expires_in': expiresIn,
            });
    }
    if (key == _decisionPath) {
      status = text.contains('"decision":"approve"') ? 'approved' : 'denied';
      return recordJson(_row());
    }
    if (method == 'GET' && target.startsWith('/v1/chat/approvals')) {
      return inboxJson([_row()]);
    }
    throw const GatewayError(404, '{"detail":"not found"}');
  }

  Map<String, Object?> _row() => {
    ...approvalRow(
      id: 'apr_1',
      status: status,
      expiresAt: soonIso(const Duration(minutes: 2)),
    ),
    'details': {
      'account': 'trabajo',
      'from': 'ana@ejemplo.com',
      'to': ['bea@ejemplo.org'],
      'cc': <String>[],
      'subject': approvedSubject,
      'body_preview': 'Hola Bea, adjunto el presupuesto.',
      'attachments': [
        {
          'name': 'presupuesto.pdf',
          'size': 12034,
          'sha256': 'abcdef0123456789',
        },
      ],
    },
  };

  Map<String, Object?> _draftJson() => {
    'account': 'trabajo',
    'draft_id': 'dft_1',
    'from': 'ana@ejemplo.com',
    'to': ['bea@ejemplo.org'],
    'cc': <String>[],
    'subject': 'Presupuesto',
    'body': 'Hola Bea, adjunto el presupuesto.',
    'content_type': 'text',
    'attachments': [
      {'name': 'presupuesto.pdf', 'size': 12034, 'sha256': 'abcdef0123456789'},
    ],
  };
}

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  group('the draft sheet answers the approval', () {
    testWidgets(
      'the approval shows once, in the footer, not as a second card',
      (tester) async {
        final host = _ApprovingHost();
        final controller = MailDraftController(
          MailApi(host),
          account: 'trabajo',
          draftId: 'dft_1',
        );
        final approvals = ApprovalsController(
          transport: host,
          deviceId: 'dev_1',
        );
        await mountWidget(
          tester,
          MailDraftScreen(controller: controller, approvals: approvals),
        );
        await tester.pump();
        await tester.tap(find.text('Enviar'));
        await tester.pump();
        await tester.pump();

        expect(find.text('Presupuesto'), findsOneWidget);
        expect(find.text('Un agente necesita permiso'), findsNothing);
        expect(find.text('Permitir una vez'), findsNothing);
        expect(find.text('Aprobar y enviar'), findsOneWidget);
        expect(find.text('Denegar'), findsOneWidget);
        await unmountWidget(tester);
        controller.dispose();
        approvals.dispose();
      },
    );

    testWidgets(
      'on a 360 by 640 screen the approve and deny buttons need no scroll',
      (tester) async {
        final host = _ApprovingHost();
        final controller = MailDraftController(
          MailApi(host),
          account: 'trabajo',
          draftId: 'dft_1',
        );
        final approvals = ApprovalsController(
          transport: host,
          deviceId: 'dev_1',
        );
        await mountWidget(
          tester,
          MailDraftScreen(controller: controller, approvals: approvals),
          size: const Size(360, 640),
        );
        await tester.pump();
        await tester.tap(find.text('Enviar'));
        await tester.pump();
        await tester.pump();

        expect(
          tester.getBottomLeft(find.text('Aprobar y enviar')).dy,
          lessThan(640),
        );
        expect(tester.getBottomLeft(find.text('Denegar')).dy, lessThan(640));
        await unmountWidget(tester);
        controller.dispose();
        approvals.dispose();
      },
    );

    testWidgets('approving sends it, and the window to cancel opens', (
      tester,
    ) async {
      final host = _ApprovingHost();
      final controller = MailDraftController(
        MailApi(host),
        account: 'trabajo',
        draftId: 'dft_1',
      );
      final approvals = ApprovalsController(transport: host, deviceId: 'dev_1');
      await mountWidget(
        tester,
        MailDraftScreen(controller: controller, approvals: approvals),
      );
      await tester.pump();
      await tester.tap(find.text('Enviar'));
      await tester.pump();
      await tester.pump();
      await tester.tap(find.text('Aprobar y enviar'));
      for (var second = 0; second < 3; second++) {
        await tester.pump(const Duration(seconds: 1));
      }

      expect(host.calls, contains(_decisionPath));
      expect(host.sends, 2);
      expect(find.text('Cancelar envío'), findsOneWidget);
      expect(find.textContaining('Se envía en'), findsOneWidget);
      await unmountWidget(tester);
      controller.dispose();
      approvals.dispose();
    });

    testWidgets('denying ends the send and nothing is sent', (tester) async {
      final host = _ApprovingHost();
      final controller = MailDraftController(
        MailApi(host),
        account: 'trabajo',
        draftId: 'dft_1',
      );
      final approvals = ApprovalsController(transport: host, deviceId: 'dev_1');
      await mountWidget(
        tester,
        MailDraftScreen(controller: controller, approvals: approvals),
      );
      await tester.pump();
      await tester.tap(find.text('Enviar'));
      await tester.pump();
      await tester.pump();
      await tester.tap(find.text('Denegar'));
      for (var second = 0; second < 3; second++) {
        await tester.pump(const Duration(seconds: 1));
      }

      expect(host.calls, contains(_decisionPath));
      expect(host.sends, 1);
      expect(find.text('Cerrar'), findsOneWidget);
      expect(find.text('Enviar'), findsNothing);
      await unmountWidget(tester);
      controller.dispose();
      approvals.dispose();
    });

    testWidgets('an expired approval lets the operator send again', (
      tester,
    ) async {
      final host = _ApprovingHost(expiresIn: 0);
      final controller = MailDraftController(
        MailApi(host),
        account: 'trabajo',
        draftId: 'dft_1',
      );
      final approvals = ApprovalsController(transport: host, deviceId: 'dev_1');
      await mountWidget(
        tester,
        MailDraftScreen(controller: controller, approvals: approvals),
      );
      await tester.pump();
      await tester.tap(find.text('Enviar'));
      await tester.pump();
      await tester.pump(const Duration(seconds: 1));

      expect(find.textContaining('caducó'), findsOneWidget);
      expect(find.text('Enviar'), findsOneWidget);
      await tester.tap(find.text('Enviar'));
      await tester.pump();
      expect(host.sends, 2);
      await unmountWidget(tester);
      controller.dispose();
      approvals.dispose();
    });

    testWidgets('a divergent approval cannot be approved from the sheet', (
      tester,
    ) async {
      final host = _ApprovingHost(approvedSubject: 'Otro asunto');
      final controller = MailDraftController(
        MailApi(host),
        account: 'trabajo',
        draftId: 'dft_1',
      );
      final approvals = ApprovalsController(transport: host, deviceId: 'dev_1');
      await mountWidget(
        tester,
        MailDraftScreen(controller: controller, approvals: approvals),
      );
      await tester.pump();
      await tester.tap(find.text('Enviar'));
      await tester.pump();
      await tester.pump();

      expect(find.textContaining('no coincide'), findsOneWidget);
      final approve = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, 'Aprobar y enviar'),
      );
      expect(approve.onPressed, isNull);
      await tester.tap(find.text('Aprobar y enviar'), warnIfMissed: false);
      await tester.pump();
      expect(host.calls, isNot(contains(_decisionPath)));
      await unmountWidget(tester);
      controller.dispose();
      approvals.dispose();
    });
  });
}
