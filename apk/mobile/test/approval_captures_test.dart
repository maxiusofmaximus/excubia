import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/state/approvals_controller.dart';
import 'package:herdr_notify/src/rust/api/approvals/error.dart';
import 'package:herdr_notify/src/rust/api/approvals/model.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/chat/approval_card.dart';
import 'package:herdr_notify/ui/chat/approval_settings_screen.dart';
import 'package:herdr_notify/ui/chat/approval_timeline_line.dart';
import 'package:herdr_notify/ui/theme.dart';

import 'support/approval_fixtures.dart';
import 'support/scripted_transport.dart';

/// Writes PNG captures to `APK7_EVIDENCE` when that variable names a folder.
/// Without it the test only renders, so a normal `flutter test` writes nothing.
/// The countdown reads the device clock, so captures are not golden files.
final String? _evidenceDir = Platform.environment['APK7_EVIDENCE'];

Future<ByteData> _font(String path) async =>
    ByteData.sublistView(await File(path).readAsBytes());

Future<void> _loadReadableFonts() async {
  // flutter_test draws text in Ahem and icons as boxes. The app asks for the
  // Windows sans ('Segoe UI'), 'monospace' and the Material icon font; loading
  // the real faces under those names makes the captures readable.
  const icons =
      r'C:\Users\Max\flutter\bin\cache\artifacts\material_fonts\materialicons-regular.otf';
  for (final family in ['Segoe UI', 'Roboto']) {
    await (FontLoader(
      family,
    )..addFont(_font(r'C:\Windows\Fonts\segoeui.ttf'))).load();
  }
  await (FontLoader(
    'monospace',
  )..addFont(_font(r'C:\Windows\Fonts\consola.ttf'))).load();
  await (FontLoader('MaterialIcons')..addFont(_font(icons))).load();
}

ChatApproval _record({String status = 'pending', String? expiresAt}) =>
    parseApprovalRecord(
      json: recordJson(approvalRow(status: status, expiresAt: expiresAt)),
    )!;

Widget _app(GlobalKey key, Brightness brightness, Widget home) =>
    RepaintBoundary(
      key: key,
      child: MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: buildTheme(brightness),
        home: home,
      ),
    );

Widget _page(Widget child) => Scaffold(
  body: Padding(padding: const EdgeInsets.all(16), child: child),
);

Future<void> _save(WidgetTester tester, GlobalKey key, String name) async {
  final dir = _evidenceDir;
  if (dir == null) return;
  final boundary =
      key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
  final bytes = await tester.runAsync(() async {
    final image = await boundary.toImage(pixelRatio: 2);
    final data = await image.toByteData(format: ui.ImageByteFormat.png);
    return data!.buffer.asUint8List();
  });
  await tester.runAsync(() => File('$dir/$name.png').writeAsBytes(bytes!));
}

void main() {
  setUpAll(() async {
    await RustLib.init();
    if (_evidenceDir != null) await _loadReadableFonts();
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
        await _save(tester, key, '${shot.key}-$mode');
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
      await _save(tester, lineKey, 'timeline-line-$mode');

      final detailKey = GlobalKey();
      await tester.pumpWidget(
        _app(detailKey, brightness, _page(Column(children: [line]))),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Tú aprobaste: cargo test'));
      await tester.pumpAndSettle();
      await _save(tester, detailKey, 'timeline-detail-$mode');
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
      await _save(tester, settingsKey, 'settings-global-and-project-$mode');
      await tester.pumpWidget(const SizedBox());
      approvals.dispose();
    });
  }
}
