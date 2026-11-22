import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/chat_transport.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/mail_api.dart';
import 'package:excubia/ui/mail/mail_settings_screen.dart';
import 'package:excubia/ui/theme.dart';

/// A host that answers by exact route ("METHOD target"). Anything it was not
/// told about is a 404, so a stray call shows up as a failure.
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
    if (answer is HostError) throw answer;
    if (answer is String) return answer;
    throw const HostError(404, '{"detail":"not found"}');
  }
}

String _ok(Map<String, Object?> fields) => jsonEncode({'ok': true, ...fields});

const String _accounts = '/v1/mail/accounts';
const String _loginStart = 'POST /v1/mail/login/start';
const String _loginStatus = 'GET /v1/mail/login/status?account=trabajo';

Map<String, Object> _linked({bool sendEnabled = false}) => {
  'GET $_accounts': _ok({
    'accounts': [
      {
        'label': 'trabajo',
        'tenant': 'organizations',
        'upn': 'ana@ejemplo.com',
        'send_enabled': sendEnabled,
        'access_token': 'TOKEN-SECRETO',
      },
    ],
  }),
  _loginStart: _ok({
    'login_id': 'L1',
    'user_code': 'ABCD-1234',
    'verification_uri': 'https://microsoft.com/devicelogin',
    'expires_in': 900,
    'interval': 5,
    'device_code': 'DEVICE-SECRETO',
  }),
  _loginStatus: _ok({
    'login': {'status': 'pending'},
  }),
};

/// Gives the screen room, so every row is built and can be tapped.
void _tallSurface(WidgetTester tester) {
  tester.view.physicalSize = const Size(800, 3200);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
}

Future<void> _mount(WidgetTester tester, _Host host) async {
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      home: MailSettingsScreen(mail: MailApi(host)),
    ),
  );
  await tester.pump();
}

Future<void> _unmount(WidgetTester tester) async {
  // The code card ticks while it is open; leaving the tree stops it.
  await tester.pumpWidget(const SizedBox());
}

void main() {
  testWidgets('without the admin permission it says why and offers no connect', (
    tester,
  ) async {
    _tallSurface(tester);
    await _mount(tester, _Host({'GET $_accounts': const HostError(403, '')}));

    expect(find.textContaining('no tiene permiso'), findsOneWidget);
    expect(find.text('Conectar una cuenta'), findsNothing);
  });

  testWidgets('lists accounts by label and address, never a token', (
    tester,
  ) async {
    _tallSurface(tester);
    await _mount(tester, _Host(_linked()));

    expect(find.text('trabajo'), findsOneWidget);
    expect(
      find.text('ana@ejemplo.com · Cuenta de trabajo o escuela'),
      findsOneWidget,
    );
    expect(find.textContaining('TOKEN-SECRETO'), findsNothing);
  });

  testWidgets('connecting shows the code and the page, never the device code', (
    tester,
  ) async {
    _tallSurface(tester);
    final host = _Host(_linked());
    await _mount(tester, host);

    await tester.tap(find.text('Conectar una cuenta'));
    await tester.pump(const Duration(milliseconds: 300));
    await tester.enterText(find.byType(TextField), 'trabajo');
    await tester.tap(find.widgetWithText(FilledButton, 'Conectar'));
    await tester.pump();
    await tester.pump();

    expect(host.calls, contains(_loginStart));
    expect(find.text('ABCD-1234'), findsOneWidget);
    expect(find.text('https://microsoft.com/devicelogin'), findsOneWidget);
    expect(find.text('Copiar código'), findsOneWidget);
    expect(find.textContaining('DEVICE-SECRETO'), findsNothing);
    expect(find.textContaining('Caduca en '), findsOneWidget);
    await _unmount(tester);
  });

  testWidgets('checking once while the code is unused says so and keeps the code', (
    tester,
  ) async {
    _tallSurface(tester);
    final host = _Host(_linked());
    await _mount(tester, host);
    await tester.tap(find.text('Conectar una cuenta'));
    await tester.pump(const Duration(milliseconds: 300));
    await tester.enterText(find.byType(TextField), 'trabajo');
    await tester.tap(find.widgetWithText(FilledButton, 'Conectar'));
    await tester.pump();
    await tester.pump();

    await tester.tap(find.text('Ya lo he hecho, comprobar'));
    await tester.pump();
    await tester.pump();

    expect(host.calls.where((c) => c == _loginStatus), hasLength(1));
    expect(find.textContaining('Todavía no has introducido'), findsOneWidget);
    expect(find.text('ABCD-1234'), findsOneWidget);
    await _unmount(tester);
  });

  testWidgets('a host error becomes a Spanish sentence, never the raw code', (
    tester,
  ) async {
    _tallSurface(tester);
    final answers = _linked()
      ..[_loginStatus] = _ok({
        'login': {'status': 'error', 'code': 'mail_auth_failed'},
      });
    await _mount(tester, _Host(answers));
    await tester.tap(find.text('Conectar una cuenta'));
    await tester.pump(const Duration(milliseconds: 300));
    await tester.enterText(find.byType(TextField), 'trabajo');
    await tester.tap(find.widgetWithText(FilledButton, 'Conectar'));
    await tester.pump();
    await tester.pump();

    await tester.tap(find.text('Ya lo he hecho, comprobar'));
    await tester.pump();
    await tester.pump();

    expect(find.textContaining('volver a conectar'), findsOneWidget);
    expect(find.textContaining('mail_'), findsNothing);
    await _unmount(tester);
  });

  testWidgets('the send switch shows what the host says, never a guess', (
    tester,
  ) async {
    _tallSurface(tester);
    await _mount(tester, _Host(_linked(sendEnabled: true)));

    expect(tester.widget<Switch>(find.byType(Switch)).value, isTrue);
  });

  testWidgets('revoking asks first; cancelling sends nothing', (tester) async {
    _tallSurface(tester);
    final host = _Host(_linked(sendEnabled: true));
    await _mount(tester, host);

    await tester.tap(find.byType(Switch));
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('Revocar el envío de «trabajo»'), findsOneWidget);
    await tester.tap(find.text('Cancelar'));
    await tester.pump(const Duration(milliseconds: 300));

    expect(host.calls, isNot(contains('POST /v1/mail/send/disable')));
  });

  testWidgets('revoking after confirming calls the host once', (tester) async {
    _tallSurface(tester);
    final host = _Host({
      ..._linked(sendEnabled: true),
      'POST /v1/mail/send/disable': _ok({
        'account': 'trabajo',
        'send_enabled': false,
      }),
    });
    await _mount(tester, host);

    await tester.tap(find.byType(Switch));
    await tester.pump(const Duration(milliseconds: 300));
    await tester.tap(find.widgetWithText(FilledButton, 'Revocar'));
    await tester.pump();
    await tester.pump();

    expect(
      host.calls.where((c) => c == 'POST /v1/mail/send/disable'),
      hasLength(1),
    );
    expect(find.text('Envío revocado para «trabajo».'), findsOneWidget);
  });

  testWidgets('the screen starts no draft and no send on its own', (
    tester,
  ) async {
    _tallSurface(tester);
    final host = _Host(_linked());
    await _mount(tester, host);
    await tester.tap(find.byType(Switch));
    await tester.pump();
    await tester.pump();

    expect(
      host.calls.where((c) => c.contains('/drafts') || c.contains('/sends')),
      isEmpty,
    );
    await _unmount(tester);
  });
}
