import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/alerts/alert_status.dart';
import 'package:excubia/core/api/alerts_api.dart';
import 'package:excubia/ui/alerts/alert_rules_screen.dart';
import 'package:excubia/ui/alerts/alerts_setup_section.dart';
import 'package:excubia/ui/theme.dart';

import '../support/scripted_transport.dart';
import '../support/widget_mount.dart';
import 'alert_fakes.dart';

AlertStatus statusWith({
  bool granted = true,
  Map<String, bool> channels = const {
    'agents': true,
    'agents-review': true,
    'recordatorios': true,
    'conexion': true,
  },
  bool battery = true,
  bool running = false,
  bool enabled = false,
  String manufacturer = 'samsung',
  bool autostart = false,
  bool bootMissed = false,
}) => AlertStatus(
  notificationsGranted: granted,
  channels: channels,
  batteryExempt: battery,
  serviceRunning: running,
  enabled: enabled,
  lastExit: 'cerrada por el usuario',
  manufacturer: manufacturer,
  fgsType: 'specialUse',
  autostartScreen: autostart,
  bootMissed: bootMissed,
);

void main() {
  late FakeAlertPlatform platform;

  setUp(() => platform = FakeAlertPlatform());

  Future<void> pump(WidgetTester tester, {AlertsApi? rules}) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Scaffold(
          body: SingleChildScrollView(
            child: AlertsSetupSection(
              platform: platform,
              rules: rules ?? AlertsApi(ScriptedTransport({})),
            ),
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();
  }

  testWidgets('the rules entry opens the alert rules screen', (tester) async {
    platform.reported = statusWith();
    await pump(
      tester,
      rules: AlertsApi(
        ScriptedTransport({
          'GET /v1/alerts/rules': okJson({'rules': []}),
          'GET /v1/alerts/mode': okJson({'mode': 'all'}),
          'GET /v1/state': jsonEncode({'agents': []}),
        }),
      ),
    );

    final entry = find.text('Reglas de aviso');
    await tester.ensureVisible(entry);
    await tester.tap(entry);
    await tester.pumpAndSettle();

    expect(find.byType(AlertRulesScreen), findsOneWidget);
  });

  testWidgets('a denied permission says so and asks for it', (tester) async {
    platform.reported = statusWith(granted: false);
    await pump(tester);

    expect(find.text('Denegado'), findsOneWidget);
    await tester.tap(find.text('Permitir'));
    await tester.pump();
    expect(platform.opened, ['permission']);
  });

  testWidgets('each channel shows whether it is on and opens its settings', (
    tester,
  ) async {
    platform.reported = statusWith(
      channels: {'agents': true, 'agents-review': false, 'conexion': true},
    );
    await pump(tester);

    expect(find.text('Aprobaciones y preguntas: activo'), findsOneWidget);
    expect(find.text('Tareas terminadas: desactivado'), findsOneWidget);
    expect(find.text('Conexión con el PC: activo'), findsOneWidget);

    final settings = find.text('Ajustes').at(1);
    await tester.ensureVisible(settings);
    await tester.tap(settings);
    await tester.pump();
    expect(platform.opened, ['channel:agents-review']);
  });

  testWidgets(
    'a restricted battery says it may stop the alerts and opens its settings',
    (tester) async {
      platform.reported = statusWith(battery: false);
      await pump(tester);

      expect(
        find.text('Optimizada: Android puede parar los avisos'),
        findsOneWidget,
      );
      final button = find.text('Abrir ajustes de batería');
      await tester.ensureVisible(button);
      await tester.tap(button);
      await tester.pump();
      expect(platform.opened, ['battery']);
    },
  );

  testWidgets(
    'the service switch starts off, shows its state and sends the change',
    (tester) async {
      platform.reported = statusWith(running: false, enabled: false);
      await pump(tester);

      expect(find.text('Detenido'), findsOneWidget);
      final toggle = find.byType(Switch);
      await tester.ensureVisible(toggle);
      await tester.tap(toggle);
      await tester.pump();
      expect(platform.enabledCalls, [true]);
    },
  );

  testWidgets('the last stop is shown in Spanish', (tester) async {
    platform.reported = statusWith();
    await pump(tester);

    expect(find.text('Última parada: cerrada por el usuario'), findsOneWidget);
  });

  testWidgets('the force-stop and reboot limits are explained', (tester) async {
    platform.reported = statusWith();
    await pump(tester);

    expect(find.textContaining('fuerzas la detención'), findsOneWidget);
    expect(find.textContaining('cuando lo desbloqueas'), findsOneWidget);
  });

  testWidgets(
    'a TECNO phone gets the three steps and one app settings button',
    (tester) async {
      platform.reported = statusWith(manufacturer: 'tecno');
      await pump(tester);

      expect(find.textContaining('escribe tu PIN'), findsOneWidget);
      expect(
        find.textContaining('pueden variar según la versión'),
        findsOneWidget,
      );
      final button = find.text('Abrir ajustes de esta app');
      await tester.ensureVisible(button);
      await tester.tap(button);
      await tester.pump();
      expect(platform.opened, ['app']);
    },
  );

  testWidgets('other makers get no maker steps', (tester) async {
    platform.reported = statusWith(manufacturer: 'samsung');
    await pump(tester);

    expect(find.textContaining('escribe tu PIN'), findsNothing);
    expect(find.textContaining('pueden variar según la versión'), findsNothing);
  });

  testWidgets('with the autostart screen, its button opens it', (tester) async {
    platform.reported = statusWith(autostart: true);
    await pump(tester);

    expect(find.text('Abrir ajustes de esta app'), findsNothing);
    final button = find.text('Abrir inicio automático');
    await tester.ensureVisible(button);
    await tester.tap(button);
    await tester.pump();
    expect(platform.opened, ['autostart']);
  });

  testWidgets('without the autostart screen, the row opens the app settings', (
    tester,
  ) async {
    platform.reported = statusWith();
    await pump(tester);

    expect(find.text('Abrir inicio automático'), findsNothing);
    expect(find.text('Abrir ajustes de esta app'), findsOneWidget);
  });

  testWidgets('a boot the app missed shows a warning above the autostart row', (
    tester,
  ) async {
    platform.reported = statusWith(autostart: true, bootMissed: true);
    await pump(tester);

    expect(find.textContaining('no volvieron solos'), findsOneWidget);
  });

  testWidgets('a boot that was seen shows no warning', (tester) async {
    platform.reported = statusWith(autostart: true);
    await pump(tester);

    expect(find.textContaining('no volvieron solos'), findsNothing);
  });

  testWidgets(
    'the list is titled as the phone requirements, and the switch is not experimental',
    (tester) async {
      platform.reported = statusWith();
      await pump(tester);

      expect(find.text('Lo que necesita tu móvil'), findsOneWidget);
      expect(find.text('Avisos en segundo plano'), findsOneWidget);
      expect(find.textContaining('experimental'), findsNothing);
    },
  );

  testWidgets('copying the diagnostics puts the log on the clipboard', (
    tester,
  ) async {
    String? copied;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copied = (call.arguments as Map)['text'] as String?;
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );
    platform.log = '2026-10-28 10:00:00 servicio parado\n';
    await pump(tester);

    final button = find.text('Copiar diagnóstico');
    await tester.ensureVisible(button);
    await tester.tap(button);
    await tester.pumpAndSettle();

    expect(copied, platform.log);
    expect(find.text('Diagnóstico copiado'), findsOneWidget);
  });
}
