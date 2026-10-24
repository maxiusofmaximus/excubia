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
  ScriptedTransport host,
) async {
  final approvals = ApprovalsController(transport: host, deviceId: 'dev_1');
  await approvals.refresh();
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      home: Scaffold(
        body: ApprovalInbox(controller: approvals, projects: _projects),
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
    expect(find.text('Permitida una vez'), findsOneWidget);
    expect(find.text('Permitir una vez'), findsNothing);
    expect(find.text('Pendientes de ti'), findsNothing);
    expect(find.text('Aprobaciones recientes'), findsOneWidget);
    approvals.dispose();
  });
}
