import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/agenda_api.dart';
import 'package:herdr_notify/core/api/alerts_api.dart';
import 'package:herdr_notify/core/api/chat_transport.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/api/mail_api.dart';
import 'package:herdr_notify/core/api/phone_contacts.dart';
import 'package:herdr_notify/core/app_info.dart';
import 'package:herdr_notify/core/state/voice_send_settings.dart';
import 'package:herdr_notify/ui/settings_screen.dart';
import 'package:herdr_notify/ui/theme.dart';

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

void main() {
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
        onUnpaired: () async {},
      ),
    );

    expect(find.text('Excubia'), findsOneWidget);
    expect(find.text('Vigila a tus agentes mientras no estás'), findsOneWidget);
    expect(find.textContaining('Versión $appVersion'), findsOneWidget);
    expect(find.text('Avisos con la pantalla apagada'), findsOneWidget);
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
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: SettingsScreen(
          mail: MailApi(_NoCallTransport()),
          voice: VoiceSendSettings(),
          agenda: AgendaApi(_NoCallTransport()),
          phone: const ChannelPhoneContacts(),
          alertRules: AlertsApi(_NoCallTransport()),
          onUnpaired: () async {},
        ),
      ),
    );

    expect(find.text('CORREO'), findsOneWidget);
    expect(find.text('Cuentas y envío'), findsOneWidget);
  });
}

SettingsScreen _settings(Future<void> Function() onUnpaired) => SettingsScreen(
  mail: MailApi(_NoCallTransport()),
  voice: VoiceSendSettings(),
  agenda: AgendaApi(_NoCallTransport()),
  phone: const ChannelPhoneContacts(),
  alertRules: AlertsApi(_NoCallTransport()),
  onUnpaired: onUnpaired,
);
