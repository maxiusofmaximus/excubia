import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/core/state/approvals_controller.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';
import 'package:herdr_notify/ui/chat/approval_settings_screen.dart';
import 'package:herdr_notify/ui/theme.dart';

import 'support/approval_fixtures.dart';
import 'support/scripted_transport.dart';

const _global = 'Pedirme siempre a mí';
const _settings = 'GET /v1/chat/approvals/settings';
const _write = 'POST /v1/chat/approvals/settings';

const _projects = [
  ChatProject(id: 'prj_a', name: 'Demo', rootLabel: 'demo', status: 'idle'),
];

Future<ApprovalsController> _pump(
  WidgetTester tester,
  ScriptedTransport host,
) async {
  final approvals = ApprovalsController(transport: host, deviceId: 'dev_1');
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      home: ApprovalSettingsScreen(
        approvals: approvals,
        conversationId: null,
        projects: _projects,
      ),
    ),
  );
  await tester.pumpAndSettle();
  return approvals;
}

SwitchListTile _switch(WidgetTester tester, String title) =>
    tester.widget<SwitchListTile>(find.widgetWithText(SwitchListTile, title));

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  testWidgets(
    'the global switch and the project override show the host settings',
    (tester) async {
      final host = ScriptedTransport({
        _settings: settingsJson(
          global: 'always_user',
          projects: [
            {'project_id': 'prj_a', 'mode': 'delegated'},
          ],
        ),
        _write: settingsJson(global: 'always_user'),
      });
      final approvals = await _pump(tester, host);
      expect(_switch(tester, _global).value, isTrue);
      expect(find.text('Demo'), findsOneWidget);
      expect(find.text('Ajuste propio'), findsOneWidget);
      expect(_switch(tester, 'Demo').value, isFalse);
      expect(find.textContaining('Con el ajuste activo'), findsOneWidget);
      approvals.dispose();
    },
  );

  testWidgets('turning the global switch off says who decides and asks first', (
    tester,
  ) async {
    final host = ScriptedTransport({
      _settings: settingsJson(global: 'always_user'),
      _write: settingsJson(global: 'delegated'),
    });
    final approvals = await _pump(tester, host);
    await tester.tap(find.widgetWithText(SwitchListTile, _global));
    await tester.pumpAndSettle();
    expect(
      find.descendant(
        of: find.byType(AlertDialog),
        matching: find.textContaining('el lead y Alfred deciden antes'),
      ),
      findsOneWidget,
    );
    await tester.tap(find.text('Cancelar'));
    await tester.pumpAndSettle();
    expect(host.calls, isNot(contains(_write)));
    await tester.tap(find.widgetWithText(SwitchListTile, _global));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Apagar'));
    await tester.pumpAndSettle();
    expect(host.calls, contains(_write));
    approvals.dispose();
  });

  testWidgets('a project switch posts an override for that project only', (
    tester,
  ) async {
    final host = ScriptedTransport({
      _settings: settingsJson(),
      _write: settingsJson(
        projects: [
          {'project_id': 'prj_a', 'mode': 'always_user'},
        ],
      ),
    });
    final approvals = await _pump(tester, host);
    expect(find.text('Usa el ajuste global'), findsOneWidget);
    await tester.tap(find.widgetWithText(SwitchListTile, 'Demo'));
    await tester.pumpAndSettle();
    final sent = host.bodies[_write]!.single;
    expect(sent['mode'], 'always_user');
    expect(sent['project_id'], 'prj_a');
    approvals.dispose();
  });

  testWidgets(
    'without APPROVE the controls are off and the host line says why',
    (tester) async {
      final host = ScriptedTransport({_settings: settingsJson()})
        ..failNext(_settings, const GatewayError(403, forbiddenBody));
      final approvals = await _pump(tester, host);
      expect(
        find.textContaining('Este dispositivo no tiene el permiso APPROVE'),
        findsOneWidget,
      );
      expect(
        tester.widget<SelectableText>(find.byType(SelectableText)).data,
        grantCommand,
      );
      expect(_switch(tester, _global).onChanged, isNull);
      expect(_switch(tester, 'Demo').onChanged, isNull);
      expect(host.calls, isNot(contains(_write)));
      approvals.dispose();
    },
  );
}
