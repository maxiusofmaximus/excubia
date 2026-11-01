import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/chat_transport.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/core/api/mail_api.dart';
import 'package:herdr_notify/core/state/mail_draft_controller.dart';
import 'package:herdr_notify/src/rust/api/approvals/model.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/chat/approval_card.dart';
import 'package:herdr_notify/ui/chat/chat_bubbles.dart';
import 'package:herdr_notify/ui/mail/mail_code_card.dart';
import 'package:herdr_notify/ui/mail/mail_draft_screen.dart';
import 'package:herdr_notify/ui/mail/mail_facts.dart';
import 'package:herdr_notify/ui/mail/mail_send_footer.dart';
import 'package:herdr_notify/ui/theme.dart';
import 'package:herdr_notify/ui/widgets/message_panel.dart';

import 'support/approval_fixtures.dart';
import 'support/widget_mount.dart';

/// Answers by exact route ("METHOD target"); anything unknown is a 404.
class _Host implements ChatTransport {
  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError();

  @override
  Future<List<int>> callBytes(String method, String target) =>
      throw UnimplementedError();

  _Host(this.answers);

  final Map<String, Object> answers;
  final List<String> calls = [];

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    final key = '$method $target';
    calls.add(key);
    final answer = answers[key];
    if (answer is GatewayError) throw answer;
    if (answer is String) return answer;
    throw const GatewayError(404, '{"detail":"not found"}');
  }
}

/// Answers by route, in order, and the last answer repeats. A [GatewayError]
/// answer is thrown the way the real client refuses.
class _Script implements ChatTransport {
  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError();

  @override
  Future<List<int>> callBytes(String method, String target) =>
      throw UnimplementedError();

  _Script(this.answers);

  final Map<String, List<Object>> answers;

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) async {
    final queue = answers['$method $target'] ?? const <Object>[];
    if (queue.isEmpty) throw const GatewayError(404, '{"detail":"not found"}');
    final answer = queue.length > 1 ? queue.removeAt(0) : queue.first;
    if (answer is GatewayError) throw answer;
    return answer as String;
  }
}

final String _draftAnswer = okJson({
  'draft': {
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
  },
});

/// The draft «dft_1» screen, opened the way the settings screen opens it.
MailDraftScreen draftScreen(ChatTransport transport) => MailDraftScreen(
  controller: MailDraftController(
    MailApi(transport),
    account: 'trabajo',
    draftId: 'dft_1',
  ),
);

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  group('the sign-in card in the chat', () {
    testWidgets('shows the code, copies it, counts down, and has no check', (
      tester,
    ) async {
      await mountWidget(
        tester,
        Scaffold(
          body: MailCodeCard(
            title: 'Conecta tu correo · trabajo',
            verificationUri: 'https://microsoft.com/devicelogin',
            userCode: 'ABCD-1234',
            expiresAt: DateTime.now().add(const Duration(minutes: 15)),
          ),
        ),
      );

      expect(find.text('Conecta tu correo · trabajo'), findsOneWidget);
      expect(find.text('ABCD-1234'), findsOneWidget);
      expect(find.text('Copiar código'), findsOneWidget);
      expect(find.textContaining('Caduca en 14:'), findsOneWidget);
      expect(find.text('Ya lo he hecho, comprobar'), findsNothing);
      await unmountWidget(tester);
    });

    testWidgets('without a usable time it shows no countdown at all', (
      tester,
    ) async {
      await mountWidget(
        tester,
        const Scaffold(
          body: MailCodeCard(
            title: 'Conecta tu correo',
            verificationUri: 'https://microsoft.com/devicelogin',
            userCode: 'ABCD-1234',
            expiresAt: null,
          ),
        ),
      );

      expect(find.textContaining('Caduca'), findsNothing);
      expect(find.text('ABCD-1234'), findsOneWidget);
      await unmountWidget(tester);
    });

    testWidgets(
      'in the chat it keeps the same side margins as the other cards',
      (tester) async {
        const signIn = ChatEntry(
          seq: 1,
          kind: 'card',
          suggestions: [],
          body: '',
          preview: '',
          mail: MailCard(
            account: 'trabajo',
            userCode: 'ABCD-1234',
            verificationUri: 'https://microsoft.com/devicelogin',
            expiresAt: '',
          ),
          createdAt: '2026-10-25T10:00:00.000Z',
          local: false,
        );
        await mountWidget(
          tester,
          Scaffold(
            body: TimelineRow(
              entry: signIn,
              agentName: 'Alfred',
              onRetrySend: (_) {},
              onRetryTurn: (_) {},
              onOpenDelegation: (_) {},
              onOpenAgent: (_) {},
              onCancelDelegation: (_) {},
            ),
          ),
        );

        final row = tester.widget<Padding>(
          find
              .descendant(
                of: find.byType(TimelineRow),
                matching: find.byType(Padding),
              )
              .first,
        );
        expect(
          row.padding,
          const EdgeInsets.symmetric(
            horizontal: Spacing.lg,
            vertical: Spacing.xs,
          ),
        );
        await unmountWidget(tester);
      },
    );
  });

  group('the approval facts', () {
    test(
      'a short digest keeps twelve characters and a size reads in Spanish',
      () {
        expect(shortDigest('abcdef0123456789'), 'abcdef012345…');
        expect(shortDigest('abc'), 'abc');
        expect(sizeLabel(12034), '11,8 KB');
        expect(sizeLabel(512), '512 B');
      },
    );

    testWidgets(
      'a mail.send shows subject, addresses and each attachment digest',
      (tester) async {
        await mountWidget(
          tester,
          Scaffold(
            body: MailSendFacts(
              details: const MailSendDetails(
                account: 'trabajo',
                from: 'ana@ejemplo.com',
                to: ['bea@ejemplo.org'],
                cc: [],
                subject: 'Presupuesto',
                bodyPreview: 'Hola Bea',
                attachments: [
                  ApprovalAttachment(
                    name: 'presupuesto.pdf',
                    size: 12034,
                    sha256: 'abcdef0123456789',
                  ),
                ],
              ),
            ),
          ),
        );

        expect(find.text('Presupuesto'), findsOneWidget);
        expect(find.text('bea@ejemplo.org'), findsOneWidget);
        expect(find.text('ana@ejemplo.com'), findsOneWidget);
        expect(find.textContaining('SHA-256 abcdef012345…'), findsOneWidget);
        expect(find.textContaining('CC'), findsNothing);
      },
    );

    testWidgets('a mail.attach_path shows the path, size and digest', (
      tester,
    ) async {
      await mountWidget(
        tester,
        const Scaffold(
          body: MailAttachFacts(
            details: MailAttachDetails(
              path: 'D:/fuera/contrato.pdf',
              size: 512,
              sha256: 'cd34ef56ab78cd90',
            ),
          ),
        ),
      );

      expect(find.text('D:/fuera/contrato.pdf'), findsOneWidget);
      expect(find.text('512 B'), findsOneWidget);
      expect(find.text('cd34ef56ab78…'), findsOneWidget);
    });

    test('an approval covers the draft only when every shown fact matches', () {
      const draft = MailDraft(
        draftId: 'dft_1',
        from: 'ana@ejemplo.com',
        to: ['bea@ejemplo.org'],
        cc: [],
        subject: 'Presupuesto',
        body: 'Hola Bea, adjunto el presupuesto.',
        attachments: [
          MailAttachment(name: 'presupuesto.pdf', size: 12034, sha256: 'ab12'),
        ],
      );
      MailSendDetails details({
        String account = 'trabajo',
        List<String> to = const ['bea@ejemplo.org'],
        String subject = 'Presupuesto',
        String sha256 = 'ab12',
      }) => MailSendDetails(
        account: account,
        from: 'ana@ejemplo.com',
        to: to,
        cc: const [],
        subject: subject,
        bodyPreview: 'Hola Bea',
        attachments: [
          ApprovalAttachment(
            name: 'presupuesto.pdf',
            size: 12034,
            sha256: sha256,
          ),
        ],
      );

      expect(sameMailSend(draft, details(), 'trabajo'), isTrue);
      expect(sameMailSend(draft, details(subject: 'Otro'), 'trabajo'), isFalse);
      expect(
        sameMailSend(draft, details(to: ['eva@ejemplo.org']), 'trabajo'),
        isFalse,
      );
      expect(sameMailSend(draft, details(sha256: 'ff00'), 'trabajo'), isFalse);
      expect(sameMailSend(draft, details(account: 'otra'), 'trabajo'), isFalse);
    });

    testWidgets('the chat card of a mail.send names who wants to send it', (
      tester,
    ) async {
      final row = {
        ...approvalRow(),
        'tool': 'mail.send',
        'details': {
          'account': 'trabajo',
          'from': 'ana@ejemplo.com',
          'to': ['bea@ejemplo.org'],
          'cc': <String>[],
          'subject': 'Presupuesto',
          'body_preview': 'Hola Bea',
          'attachments': <Object>[],
        },
      };
      final approval = parseApprovalInbox(json: inboxJson([row]))!.single;
      await mountWidget(
        tester,
        Scaffold(
          body: ApprovalCard(
            approval: approval,
            agentName: 'Alfred',
            busy: false,
            failure: null,
            onDecide: (_) {},
          ),
        ),
      );

      expect(find.text('Alfred quiere enviar un correo'), findsOneWidget);
      expect(find.text('Alfred necesita permiso'), findsNothing);
      expect(find.text('Permitir una vez'), findsOneWidget);
    });

    testWidgets(
      'a mail card without its facts never says «comando» and cannot be approved',
      (tester) async {
        final row = {
          ...approvalRow(),
          'tool': 'mail.send',
          'payload_redacted': null,
        };
        final approval = parseApprovalInbox(json: inboxJson([row]))!.single;
        await mountWidget(
          tester,
          Scaffold(
            body: ApprovalCard(
              approval: approval,
              agentName: 'Alfred',
              busy: false,
              failure: null,
              onDecide: (_) {},
            ),
          ),
        );

        expect(find.text('Alfred quiere enviar un correo'), findsOneWidget);
        expect(find.textContaining('Falta el detalle'), findsNothing);
        expect(
          find.textContaining('No llegaron los datos del correo'),
          findsOneWidget,
        );
        final allow = tester.widget<FilledButton>(
          find.widgetWithText(FilledButton, 'Permitir una vez'),
        );
        expect(allow.onPressed, isNull);
      },
    );
  });

  group('the waiting panel for a new device', () {
    testWidgets('says what to do in plain words and retries once per tap', (
      tester,
    ) async {
      var retries = 0;
      await mountWidget(
        tester,
        Scaffold(body: AwaitingApprovalPanel(onRetry: () => retries++)),
      );

      expect(
        find.text('Esperando que aprueben este dispositivo'),
        findsOneWidget,
      );
      expect(find.textContaining('pulsa Reintentar'), findsOneWidget);
      expect(find.textContaining('device'), findsNothing);

      await tester.tap(find.text('Reintentar'));
      await tester.pump();
      expect(retries, 1);
    });
  });

  group('the draft review screen', () {
    const draftRoute = 'GET /v1/mail/drafts/dft_1?account=trabajo';
    const sendRoute = 'POST /v1/mail/drafts/dft_1/send';
    const inboxRoute = 'GET /v1/chat/approvals';

    _Host host() => _Host({
      draftRoute: _draftAnswer,
      sendRoute: okJson({
        'state': 'awaiting_approval',
        'approval_id': 'apr_1',
        'expires_in': 120,
      }),
      inboxRoute: okJson({
        'approvals': [
          {'id': 'apr_1', 'status': 'pending'},
        ],
      }),
    });

    testWidgets(
      'shows what the approval will cover and sends nothing by itself',
      (tester) async {
        final transport = host();
        await mountWidget(tester, draftScreen(transport));
        await tester.pump();

        expect(find.text('Presupuesto'), findsOneWidget);
        expect(find.text('bea@ejemplo.org'), findsOneWidget);
        expect(find.textContaining('SHA-256 abcdef012345…'), findsOneWidget);
        expect(transport.calls, isNot(contains(sendRoute)));
        await unmountWidget(tester);
      },
    );

    testWidgets('a draft with no sender shows the connected account', (
      tester,
    ) async {
      final transport = _Host({
        draftRoute: okJson({
          'draft': {
            'account': 'trabajo',
            'draft_id': 'dft_1',
            'from': '  ',
            'to': ['bea@ejemplo.org'],
            'cc': <String>[],
            'subject': 'Presupuesto',
            'body': 'Hola Bea',
            'content_type': 'text',
            'attachments': <Object>[],
          },
        }),
      });
      await mountWidget(tester, draftScreen(transport));
      await tester.pump();

      // The account label fills «Remitente» and «Cuenta» alike.
      expect(find.text('REMITENTE'), findsOneWidget);
      expect(find.text('trabajo'), findsNWidgets(2));
      await unmountWidget(tester);
    });

    testWidgets('«Enviar» asks for approval and says where to give it', (
      tester,
    ) async {
      final transport = host();
      // A send starts a timer, so this test ends the controller it made.
      final controller = MailDraftController(
        MailApi(transport),
        account: 'trabajo',
        draftId: 'dft_1',
      );
      await mountWidget(tester, MailDraftScreen(controller: controller));
      await tester.pump();

      await tester.tap(find.text('Enviar'));
      await tester.pump();
      await tester.pump();

      expect(transport.calls.where((c) => c == sendRoute), hasLength(1));
      expect(
        find.textContaining('quiere enviar este correo a bea@ejemplo.org'),
        findsOneWidget,
      );
      expect(find.text('Enviar'), findsNothing);
      await unmountWidget(tester);
      controller.dispose();
    });

    testWidgets('«No enviar» closes the idle sheet once and never mid-send', (
      tester,
    ) async {
      final controller = MailDraftController(
        MailApi(host()),
        account: 'trabajo',
        draftId: 'dft_1',
      );
      await controller.load();
      var closed = 0;
      await mountWidget(
        tester,
        Scaffold(
          body: MailSendFooter(
            controller: controller,
            approvals: null,
            agentName: 'Alfred',
            onEdit: () {},
            onClose: () => closed++,
            onRequestChanges: () {},
          ),
        ),
      );

      expect(find.text('No enviar'), findsOneWidget);
      await tester.tap(find.text('No enviar'));
      expect(closed, 1);

      await controller.requestSend();
      await tester.pump();
      expect(controller.phase, MailSendPhase.awaitingApproval);
      expect(find.text('No enviar'), findsNothing);
      await unmountWidget(tester);
      controller.dispose();
    });

    testWidgets('a host refusal reads as a sentence, never a raw code', (
      tester,
    ) async {
      final transport = _Host({
        draftRoute: const GatewayError(
          400,
          '{"detail":{"code":"mail_unknown_draft","message":"x"}}',
        ),
      });
      await mountWidget(tester, draftScreen(transport));
      await tester.pump();

      expect(find.textContaining('no lo creó este equipo'), findsOneWidget);
      expect(find.textContaining('mail_'), findsNothing);
      await unmountWidget(tester);
    });

    /// Mounts the draft, asks for the send, and lets the approved send end as
    /// the host says [failure]. The timer runs on the fake clock of the test.
    Future<MailDraftController> failedSend(
      WidgetTester tester,
      Map<String, Object?> failure, {
      VoidCallback? onOpenSettings,
    }) async {
      final transport = _Script({
        draftRoute: [_draftAnswer],
        sendRoute: [
          okJson({
            'state': 'awaiting_approval',
            'approval_id': 'apr_1',
            'expires_in': 120,
          }),
          okJson({'state': 'pending', 'send_id': 'snd_1', 'window_seconds': 0}),
        ],
        inboxRoute: [
          okJson({
            'approvals': [
              {'id': 'apr_1', 'status': 'approved'},
            ],
          }),
        ],
        'GET /v1/mail/sends/snd_1': [
          okJson({'state': 'failed', 'failure': failure}),
        ],
      });
      final controller = MailDraftController(
        MailApi(transport),
        account: 'trabajo',
        draftId: 'dft_1',
        tick: const Duration(seconds: 1),
      );
      addTearDown(controller.dispose);
      await mountWidget(
        tester,
        MailDraftScreen(controller: controller, onOpenSettings: onOpenSettings),
      );
      await tester.pump();
      await tester.tap(find.text('Enviar'));
      for (var i = 0; i < 12 && !controller.finished; i++) {
        await tester.pump(const Duration(seconds: 1));
      }
      await tester.pump();
      return controller;
    }

    testWidgets(
      'a failure that may have sent says why, live, and offers no edit',
      (tester) async {
        final controller = await failedSend(tester, {
          'code': 'interrupted',
          'retry': 'check_sent',
        });

        const reason =
            'El envío no se completó y puede que el correo haya salido. Revisa Enviados antes de volver a intentarlo.';
        expect(controller.finished, isTrue);
        expect(find.text(reason), findsOneWidget);
        expect(
          find.ancestor(
            of: find.text(reason),
            matching: find.byWidgetPredicate(
              (widget) =>
                  widget is Semantics && widget.properties.liveRegion == true,
            ),
          ),
          findsOneWidget,
        );
        expect(find.text('Editar borrador'), findsNothing);
        expect(find.text('Editar'), findsNothing);
        expect(find.text('Pide la aprobación otra vez'), findsNothing);
        expect(find.text('Enviar'), findsNothing);
        expect(find.text('No enviar'), findsNothing);
        await unmountWidget(tester);
      },
    );

    testWidgets('a failure that did not send offers the edit and asks again', (
      tester,
    ) async {
      await failedSend(tester, {'code': 'draft_changed', 'retry': 'safe'});

      expect(
        find.text(
          'El borrador cambió después de aprobarlo. No se ha enviado nada; pide el envío otra vez.',
        ),
        findsOneWidget,
      );
      expect(find.text('Pide la aprobación otra vez'), findsOneWidget);
      expect(find.text('Editar borrador'), findsOneWidget);
      expect(find.text('Enviar'), findsNothing);
      await unmountWidget(tester);
    });

    testWidgets('an expired login opens Ajustes › Correo from the sheet', (
      tester,
    ) async {
      var opened = false;
      await failedSend(tester, {
        'code': 'auth_expired',
        'retry': 'no',
      }, onOpenSettings: () => opened = true);

      expect(
        find.textContaining('Vuelve a conectar la cuenta'),
        findsOneWidget,
      );
      expect(find.text('Editar borrador'), findsNothing);
      await tester.tap(find.text('Ir a Ajustes › Correo'));
      expect(opened, isTrue);
      await unmountWidget(tester);
    });
  });
}
