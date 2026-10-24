import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/state/approvals_controller.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/chat/approval_timeline_line.dart';
import 'package:herdr_notify/ui/theme.dart';

import 'support/approval_fixtures.dart';
import 'support/scripted_transport.dart';

ChatEntry _entry({
  String summary = 'Política aprobó: cargo test',
  String level = 'policy',
}) => ChatEntry(
  seq: 5,
  kind: 'approval',
  body: '',
  preview: summary,
  summary: summary,
  status: 'approved',
  approvalId: 'apr_1',
  level: level,
  createdAt: '2026-10-23T10:01:00.000Z',
  local: false,
);

Widget _host(Widget child) => MaterialApp(
  theme: buildTheme(),
  home: Scaffold(body: Column(children: [child])),
);

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  testWidgets('a decision below the user is one quiet line', (tester) async {
    await tester.pumpWidget(
      _host(
        ApprovalTimelineLine(
          entry: _entry(),
          controller: null,
          conversationId: 'cnv_1',
          projects: const [],
        ),
      ),
    );
    expect(find.text('Política aprobó: cargo test'), findsOneWidget);
    expect(find.byIcon(Icons.check_circle_outline), findsOneWidget);
    expect(find.byIcon(Icons.notifications), findsNothing);
  });

  testWidgets('tapping the line opens the levels, without a controller', (
    tester,
  ) async {
    await tester.pumpWidget(
      _host(
        ApprovalTimelineLine(
          entry: _entry(),
          controller: null,
          conversationId: 'cnv_1',
          projects: const [],
        ),
      ),
    );
    await tester.tap(find.text('Política aprobó: cargo test'));
    await tester.pumpAndSettle();
    expect(find.text('Cadena de niveles'), findsOneWidget);
    expect(find.text('Política: decidió alguien'), findsOneWidget);
    expect(find.text('Lead: no llegó'), findsOneWidget);
    expect(find.text('Alfred: no llegó'), findsOneWidget);
    expect(find.text('Tú: no llegó'), findsOneWidget);
    expect(find.text('No se pudo cargar el detalle.'), findsOneWidget);
  });

  testWidgets(
    'the detail reads the host record: levels passed and audit rows',
    (tester) async {
      final row = approvalRow(status: 'approved', level: 'user')
        ..['decided_at'] = '2026-10-23T10:01:00.000Z'
        ..['audit'] = [
          {
            'device_id': 'dev_1',
            'decision': 'approve',
            'decided_at': '2026-10-23T10:01:00.000Z',
          },
        ];
      final host = ScriptedTransport({
        'GET /v1/chat/approvals': jsonEncode({
          'ok': true,
          'approvals': [row],
        }),
      });
      final approvals = ApprovalsController(transport: host, deviceId: 'dev_1');
      await tester.pumpWidget(
        _host(
          ApprovalTimelineLine(
            entry: _entry(level: 'user', summary: 'Tú aprobaste: cargo test'),
            controller: approvals,
            conversationId: 'cnv_1',
            projects: const [],
          ),
        ),
      );
      await tester.tap(find.text('Tú aprobaste: cargo test'));
      await tester.pumpAndSettle();
      expect(find.text('Política: pasó sin decidir'), findsOneWidget);
      expect(find.text('Lead: pasó sin decidir'), findsOneWidget);
      expect(find.text('Alfred: pasó sin decidir'), findsOneWidget);
      expect(find.text('Tú: decidió tú'), findsOneWidget);
      expect(
        find.textContaining('approve · 2026-10-23T10:01:00.000Z · dev_1'),
        findsOneWidget,
      );
      approvals.dispose();
    },
  );
}
