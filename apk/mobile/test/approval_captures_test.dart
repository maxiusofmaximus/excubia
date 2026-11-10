import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/state/approvals_controller.dart';
import 'package:excubia/src/rust/api/approvals/error.dart';
import 'package:excubia/src/rust/api/approvals/model.dart';
import 'package:excubia/src/rust/api/chat/model.dart';
import 'package:excubia/src/rust/api/chat/model_team.dart';
import 'package:excubia/src/rust/frb_generated.dart';
import 'package:excubia/ui/chat/approval_card.dart';
import 'package:excubia/ui/chat/approval_settings_screen.dart';
import 'package:excubia/ui/chat/approval_timeline_line.dart';
import 'package:excubia/ui/theme.dart';

import 'support/approval_fixtures.dart';
import 'support/captures.dart';
import 'support/scripted_transport.dart';

/// Writes PNG captures to `APK7_EVIDENCE` when that variable names a folder.
/// Without it the test only renders, so a normal `flutter test` writes nothing.
/// The countdown reads the device clock, so captures are not golden files.
final String? _evidenceDir = evidenceDir('APK7_EVIDENCE');

ChatApproval _record({String status = 'pending', String? expiresAt}) =>
    parseApprovalRecord(
      json: recordJson(approvalRow(status: status, expiresAt: expiresAt)),
    )!;

Widget _app(GlobalKey key, Brightness brightness, Widget home) =>
    RepaintBoundary(
      key: key,
      child: MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: buildTheme(
          brightness == Brightness.dark
              ? HerdrPalettes.noche
              : HerdrPalettes.pergamino,
        ),
        home: home,
      ),
    );

Widget _page(Widget child) => Scaffold(
  body: Padding(padding: const EdgeInsets.all(16), child: child),
);

void main() {
  setUpAll(() async {
    await RustLib.init();
    if (_evidenceDir != null) await loadReadableFonts();
  });

  for (final brightness in [Brightness.light, Brightness.dark]) {
    final mode = brightness.name;

    testWidgets('$mode captures: pending, resolved, expired and 403 cards', (
      tester,
    ) async {
      await tester.binding.setSurfaceSize(const Size(360, 560));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final shots = <String, Widget>{
        'card-pending': ApprovalCard(
          approval: _record(
            expiresAt: soonIso(const Duration(minutes: 4, seconds: 40)),
          ),
          agentName: 'Bob',
          projectName: 'Demo',
          busy: false,
          failure: null,
          onDecide: (_) {},
        ),
        'card-resolved': ApprovalCard(
          approval: _record(status: 'approved'),
          agentName: 'Bob',
          projectName: 'Demo',
          busy: false,
          failure: null,
          onDecide: (_) {},
        ),
        'card-expired': ApprovalCard(
          approval: _record(status: 'expired'),
          agentName: 'Bob',
          projectName: 'Demo',
          busy: false,
          failure: null,
          onDecide: (_) {},
        ),
        'card-403': ApprovalCard(
          approval: _record(),
          agentName: 'Bob',
          projectName: 'Demo',
          busy: false,
          failure: const ApprovalFailure(
            kind: ApprovalFailureKind.needsGrant,
            command: grantCommand,
            code: 'forbidden',
          ),
          onDecide: (_) {},
        ),
      };
      for (final shot in shots.entries) {
        final key = GlobalKey();
        await tester.pumpWidget(_app(key, brightness, _page(shot.value)));
        await tester.pump();
        await saveCapture(tester, key, _evidenceDir, '${shot.key}-$mode');
        await tester.pumpWidget(const SizedBox());
      }
    });

    testWidgets('$mode captures: timeline line, its detail and the settings', (
      tester,
    ) async {
      await tester.binding.setSurfaceSize(const Size(360, 640));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final host = ScriptedTransport({
        'GET /v1/chat/approvals': inboxJson([
          approvalRow(status: 'approved', level: 'user')
            ..['audit'] = [
              {
                'device_id': 'dev_1',
                'decision': 'approve',
                'decided_at': '2026-10-23T10:01:00.000Z',
              },
            ],
        ]),
        'GET /v1/chat/approvals/settings': settingsJson(
          global: 'always_user',
          projects: [
            {'project_id': 'prj_a', 'mode': 'delegated'},
          ],
        ),
      });
      final approvals = ApprovalsController(transport: host, deviceId: 'dev_1');
      const projects = [
        ChatProject(
          id: 'prj_a',
          name: 'Demo',
          rootLabel: 'demo',
          status: 'idle',
        ),
        ChatProject(
          id: 'prj_b',
          name: 'Otro',
          rootLabel: 'otro',
          status: 'idle',
        ),
      ];
      final line = ApprovalTimelineLine(
        entry: const ChatEntry(
          seq: 5,
          kind: 'approval',
          suggestions: [],
          body: '',
          preview: 'Tú aprobaste: cargo test',
          summary: 'Tú aprobaste: cargo test',
          status: 'approved',
          approvalId: 'apr_1',
          level: 'user',
          createdAt: '2026-10-23T10:01:00.000Z',
          local: false,
        ),
        controller: approvals,
        conversationId: 'cnv_1',
        projects: projects,
      );

      final lineKey = GlobalKey();
      await tester.pumpWidget(
        _app(lineKey, brightness, _page(Column(children: [line]))),
      );
      await tester.pumpAndSettle();
      await saveCapture(tester, lineKey, _evidenceDir, 'timeline-line-$mode');

      final detailKey = GlobalKey();
      await tester.pumpWidget(
        _app(detailKey, brightness, _page(Column(children: [line]))),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Tú aprobaste: cargo test'));
      await tester.pumpAndSettle();
      await saveCapture(tester, detailKey, _evidenceDir, 'timeline-detail-$mode');
      await tester.pumpWidget(const SizedBox());

      await approvals.loadSettings();
      final settingsKey = GlobalKey();
      await tester.pumpWidget(
        _app(
          settingsKey,
          brightness,
          ApprovalSettingsScreen(
            approvals: approvals,
            conversationId: null,
            projects: projects,
          ),
        ),
      );
      await tester.pumpAndSettle();
      await saveCapture(tester, settingsKey, _evidenceDir, 'settings-global-and-project-$mode');
      await tester.pumpWidget(const SizedBox());
      approvals.dispose();
    });
  }
}
