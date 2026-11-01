import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/state/approvals_controller.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/chat/approval_inbox.dart';
import 'package:herdr_notify/ui/theme.dart';

import 'support/approval_fixtures.dart';
import 'support/scripted_transport.dart';

const _projects = [
  ChatProject(id: 'prj_a', name: 'Demo', rootLabel: 'demo', status: 'idle'),
];
const _decide = 'POST /v1/chat/approvals/apr_seat/decision';

Future<ApprovalsController> _pump(
  WidgetTester tester,
  ScriptedTransport host, {
  ChatSeat? alfred,
}) async {
  final approvals = ApprovalsController(transport: host, deviceId: 'dev_1');
  await approvals.refresh();
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      home: Scaffold(
        body: ApprovalInbox(
          controller: approvals,
          projects: _projects,
          alfred: alfred,
        ),
      ),
    ),
  );
  // A pending card ticks every second: pumpAndSettle would never settle.
  await tester.pump(const Duration(milliseconds: 100));
  return approvals;
}

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  testWidgets('lists only the approvals that belong to no conversation', (
    tester,
  ) async {
    final host = ScriptedTransport({
      'GET /v1/chat/approvals': inboxJson([
        approvalRow(id: 'apr_seat', command: 'echo x', conversationId: null),
        approvalRow(id: 'apr_conv', command: 'cargo test'),
      ]),
      'GET /v1/chat/approvals/settings': settingsJson(),
    });
    final approvals = await _pump(tester, host);
    expect(find.text('Pendientes de ti'), findsOneWidget);
    expect(find.textContaining('echo x'), findsWidgets);
    expect(find.textContaining('cargo test'), findsNothing);
    expect(find.text('Permitir una vez'), findsOneWidget);
    approvals.dispose();
  });

  testWidgets('an approval Alfred asks for reads with his name', (
    tester,
  ) async {
    final host = ScriptedTransport({
      'GET /v1/chat/approvals': inboxJson([
        approvalRow(
          id: 'apr_alfred',
          command: 'echo x',
          conversationId: null,
          seatId: 'seat_alfred',
        ),
      ]),
      'GET /v1/chat/approvals/settings': settingsJson(),
    });
    final approvals = await _pump(
      tester,
      host,
      alfred: const ChatSeat(
        agentId: 'seat_alfred',
        name: 'Alfred',
        status: 'working',
      ),
    );
    expect(find.text('Alfred necesita permiso'), findsOneWidget);
    expect(find.text('Un agente necesita permiso'), findsNothing);
    approvals.dispose();
  });

  testWidgets('renders nothing when no approval is unassigned', (tester) async {
    final host = ScriptedTransport({
      'GET /v1/chat/approvals': inboxJson([
        approvalRow(id: 'apr_conv', command: 'cargo test'),
      ]),
      'GET /v1/chat/approvals/settings': settingsJson(),
    });
    final approvals = await _pump(tester, host);
    expect(find.text('Pendientes de ti'), findsNothing);
    expect(find.text('Permitir una vez'), findsNothing);
    approvals.dispose();
  });

  testWidgets('an answered card stays visible as closed, without buttons', (
    tester,
  ) async {
    final host = ScriptedTransport({
      'GET /v1/chat/approvals': inboxJson([
        approvalRow(id: 'apr_seat', command: 'echo x', conversationId: null),
      ]),
      'GET /v1/chat/approvals/settings': settingsJson(),
      _decide: recordJson(
        approvalRow(
          id: 'apr_seat',
          status: 'approved',
          level: 'user',
          conversationId: null,
        ),
      ),
    });
    final approvals = await _pump(tester, host);
    await tester.tap(find.text('Permitir una vez'));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump(const Duration(milliseconds: 100));
    // Answered cards wait in «Recientes», folded until the operator opens them.
    expect(find.text('Permitida una vez'), findsNothing);
    await tester.tap(find.textContaining('Recientes'));
    await tester.pump();
    expect(find.text('Permitida una vez'), findsOneWidget);
    expect(find.text('Permitir una vez'), findsNothing);
    expect(find.text('Pendientes de ti'), findsNothing);
    expect(find.text('Recientes · 1'), findsOneWidget);
    approvals.dispose();
  });

  testWidgets('pending approvals come one at a time, with a counter', (
    tester,
  ) async {
    final host = ScriptedTransport({
      'GET /v1/chat/approvals': inboxJson([
        approvalRow(id: 'apr_a', command: 'echo a', conversationId: null),
        approvalRow(id: 'apr_b', command: 'echo b', conversationId: null),
      ]),
      'GET /v1/chat/approvals/settings': settingsJson(),
    });
    final approvals = await _pump(tester, host);
    expect(find.text('Pendientes de ti · 1 de 2'), findsOneWidget);
    expect(find.text('Permitir una vez'), findsOneWidget);

    await tester.tap(find.byTooltip('Siguiente'));
    await tester.pump();
    expect(find.text('Pendientes de ti · 2 de 2'), findsOneWidget);
    expect(find.text('Permitir una vez'), findsOneWidget);
    approvals.dispose();
  });

  testWidgets('a mail.send card shows what it sends, never a missing detail', (
    tester,
  ) async {
    final row = {
      ...approvalRow(id: 'apr_m1', conversationId: null),
      'kind': 'tool',
      'tool': 'mail.send',
      'summary': 'Enviar correo a bea@ejemplo.org',
      'details': {
        'account': 'trabajo',
        'from': 'ana@ejemplo.com',
        'to': ['bea@ejemplo.org'],
        'cc': <String>[],
        'subject': 'Presupuesto',
        'body_preview': 'Hola Bea…',
        'attachments': <Object>[],
      },
    };
    final host = ScriptedTransport({
      'GET /v1/chat/approvals': inboxJson([row]),
      'GET /v1/chat/approvals/settings': settingsJson(),
    });
    final approvals = await _pump(tester, host);
    expect(find.text('Presupuesto'), findsOneWidget);
    expect(find.text('Falta el detalle del comando'), findsNothing);
    approvals.dispose();
  });
}
