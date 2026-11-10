import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/agenda_api.dart';
import 'package:excubia/core/api/alfred_backend_api.dart';
import 'package:excubia/core/api/alerts_api.dart';
import 'package:excubia/core/api/chat_transport.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/mail_api.dart';
import 'package:excubia/core/api/phone_contacts.dart';
import 'package:excubia/core/app_info.dart';
import 'package:excubia/core/state/approvals_controller.dart';
import 'package:excubia/core/state/voice_send_settings.dart';
import 'package:excubia/src/rust/frb_generated.dart';
import 'package:excubia/ui/palette/palette_controller.dart';
import 'package:excubia/ui/palette/palette_prefs.dart';
import 'package:excubia/ui/palette/palette_scope.dart';
import 'package:excubia/ui/settings_screen.dart';
import 'package:excubia/ui/theme.dart';

import 'support/captures.dart';
import 'support/memory_store.dart';
import 'support/widget_mount.dart';

/// The screen under test never calls the host until the mail entry is opened.
class _NoCallTransport implements ChatTransport {
  @override
  Future<String> postBytes(String target, List<int> body, String contentType) =>
      throw UnimplementedError();

  @override
  Future<List<int>> callBytes(String method, String target) =>
      throw UnimplementedError();

  @override
  Future<String> call(
    String method,
    String target, [
    List<int> body = const [],
  ]) => throw const HostError(0, 'sin red en este test');
}

/// Set to a folder to save the approved palettes' captures; unset, they skip.
final String? _evidence = evidenceDir('APK_REDISENO_EVIDENCE');

void main() {
  setUpAll(() async {
    await RustLib.init();
    if (_evidence != null) await loadReadableFonts();
  });

  testWidgets('Ajustes shows the name, the tagline and the version', (
    tester,
  ) async {
    await mountWidget(
      tester,
      SettingsScreen(
        mail: MailApi(_NoCallTransport()),
        voice: VoiceSendSettings(),
        agenda: AgendaApi(_NoCallTransport()),
        phone: const ChannelPhoneContacts(),
        alertRules: AlertsApi(_NoCallTransport()),
        alfred: AlfredBackendApi(_NoCallTransport()),
        onUnpaired: () async {},
      ),
    );

    expect(find.text('Excubia'), findsOneWidget);
    expect(find.text('Vigila a tus agentes mientras no estás'), findsOneWidget);
    expect(find.textContaining('Versión $appVersion'), findsOneWidget);
    expect(find.text('Avisos con la pantalla apagada'), findsOneWidget);
    expect(find.text('Paleta de colores'), findsOneWidget);
  });

  testWidgets('Ajustes abre Apariencia con la rejilla de paletas', (
    tester,
  ) async {
    final values = MemoryKeyValueStore();
    final controller = await PaletteController.load(
      prefs: PalettePrefs(values),
      hasPairing: true,
    );
    await tester.pumpWidget(
      PaletteScope(
        controller: controller,
        child: MaterialApp(theme: buildTheme(), home: _settings(() async {})),
      ),
    );

    await tester.tap(find.text('Paleta de colores'));
    await tester.pumpAndSettle();

    expect(find.widgetWithText(AppBar, 'Apariencia'), findsOneWidget);
    expect(find.text('Noche'), findsOneWidget);
    expect(find.text('Continuar'), findsNothing);
  });

  testWidgets('Ajustes has a row to pair again', (tester) async {
    await mountWidget(tester, _settings(() async {}));

    expect(find.widgetWithText(ListTile, 'Emparejar de nuevo'), findsOneWidget);
  });

  testWidgets('cancelar el diálogo no vuelve a emparejar', (tester) async {
    var unpaired = 0;
    await mountWidget(tester, _settings(() async => unpaired++));

    await tester.tap(find.widgetWithText(ListTile, 'Emparejar de nuevo'));
    await tester.pumpAndSettle();
    expect(find.text('¿Emparejar de nuevo?'), findsOneWidget);
    await tester.tap(find.text('Cancelar'));
    await tester.pumpAndSettle();

    expect(unpaired, 0);
  });

  testWidgets('confirmar el diálogo vuelve a emparejar una vez', (
    tester,
  ) async {
    var unpaired = 0;
    await mountWidget(tester, _settings(() async => unpaired++));

    await tester.tap(find.widgetWithText(ListTile, 'Emparejar de nuevo'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilledButton, 'Emparejar de nuevo'));
    await tester.pumpAndSettle();

    expect(unpaired, 1);
  });

  testWidgets('Ajustes has a Correo entry for accounts and sending', (
    tester,
  ) async {
    await mountWidget(tester, _settings(() async {}));

    expect(find.text('CORREO'), findsOneWidget);
    expect(find.text('Cuentas y envío'), findsOneWidget);
  });

  testWidgets('Ajustes muestra cada sección en el orden de la maqueta', (
    tester,
  ) async {
    await mountWidget(tester, _settings(() async {}));

    const labels = [
      'APARIENCIA',
      'AGENDA',
      'MOTOR DE ALFRED',
      'VOZ',
      'CORREO',
      'AVISOS',
      'ESTE TELÉFONO',
      'ACERCA DE',
    ];
    final tops = [
      for (final label in labels) tester.getTopLeft(find.text(label)).dy,
    ];
    expect(tops, orderedEquals(List.of(tops)..sort()));
  });

  testWidgets('Aprobaciones solo aparece con las aprobaciones del chat', (
    tester,
  ) async {
    await mountWidget(tester, _settings(() async {}));
    expect(find.text('APROBACIONES'), findsNothing);

    final approvals = ApprovalsController(
      transport: _NoCallTransport(),
      deviceId: 'dev-x',
    );
    addTearDown(approvals.dispose);
    await mountWidget(tester, _settings(() async {}, approvals: approvals));

    expect(find.text('APROBACIONES'), findsOneWidget);
    expect(find.widgetWithText(ListTile, 'Aprobaciones'), findsOneWidget);
  });

  for (final scale in [1.3, 2.0]) {
    testWidgets('Ajustes sin desbordes con la fuente al ${scale}x', (
      tester,
    ) async {
      await mountAtPhone(tester, _settings(() async {}), textScale: scale);
      expect(tester.takeException(), isNull);

      await tester.drag(find.byType(ListView), const Offset(0, -4000));
      await tester.pump();
      expect(tester.takeException(), isNull);
    });
  }

  for (final palette in [HerdrPalettes.noche, HerdrPalettes.papel]) {
    testWidgets(
      'captura: Ajustes en ${palette.name}',
      skip: _evidence == null,
      (tester) async {
        tester.view.physicalSize = const Size(390, 844);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final key = GlobalKey();
        await tester.pumpWidget(
          MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: buildTheme(palette),
            home: RepaintBoundary(key: key, child: _settings(() async {})),
          ),
        );
        await tester.pump();
        await saveCapture(
          tester,
          key,
          _evidence,
          'redis-ajustes-${palette.id}',
        );
      },
    );
  }
}

SettingsScreen _settings(
  Future<void> Function() onUnpaired, {
  ApprovalsController? approvals,
}) => SettingsScreen(
  mail: MailApi(_NoCallTransport()),
  voice: VoiceSendSettings(),
  agenda: AgendaApi(_NoCallTransport()),
  phone: const ChannelPhoneContacts(),
  alertRules: AlertsApi(_NoCallTransport()),
  alfred: AlfredBackendApi(_NoCallTransport()),
  onUnpaired: onUnpaired,
  approvals: approvals,
);
