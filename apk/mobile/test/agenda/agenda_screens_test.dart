import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/agenda_api.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/core/api/phone_contacts.dart';
import 'package:herdr_notify/ui/agenda/agenda_contact_screen.dart';
import 'package:herdr_notify/ui/agenda/agenda_import_screen.dart';
import 'package:herdr_notify/ui/agenda/agenda_notes_screen.dart';
import 'package:herdr_notify/ui/agenda/agenda_widgets.dart';

import '../support/fake_phone_contacts.dart';
import '../support/scripted_transport.dart';
import '../support/widget_mount.dart';

const String _probePath = 'POST /v1/contacts/ct_grant_probe/delete';

AgendaContact _ana() => AgendaContact.fromJson({
  'id': 'ct_1',
  'name': 'Ana',
  'aliases': <String>[],
  'addresses': [
    {'address': 'ana@mail.es', 'source': 'app'},
  ],
  'notes': '',
  'updated_at': '2026-10-29T09:00:00.000Z',
});

/// A button that opens [screen] on a route of its own, so a pop returns here.
Widget _pushedFrom(Widget screen) => Builder(
  builder: (context) => TextButton(
    onPressed: () => Navigator.of(context).push(
      MaterialPageRoute<void>(builder: (_) => screen),
    ),
    child: const Text('Abrir'),
  ),
);

void main() {
  group('notas', () {
    testWidgets('la lista muestra asunto y extracto de cada nota', (tester) async {
      final transport = ScriptedTransport({
        'GET /v1/notes': okJson({
          'notes': [
            {
              'id': 'nt_1',
              'subject': 'Cliente',
              'excerpt': 'presupuesto',
              'updated_at': '2026-10-29T09:00:00.000Z',
            },
          ],
          'next': null,
        }),
      });

      await mountWidget(tester, AgendaNotesScreen(api: AgendaApi(transport)));
      await tester.pumpAndSettle();

      expect(find.text('Cliente'), findsOneWidget);
      expect(find.textContaining('presupuesto'), findsOneWidget);
      await unmountWidget(tester);
    });

    testWidgets('sin notas dice cómo crear una', (tester) async {
      final transport = ScriptedTransport({
        'GET /v1/notes': okJson({'notes': [], 'next': null}),
      });

      await mountWidget(tester, AgendaNotesScreen(api: AgendaApi(transport)));
      await tester.pumpAndSettle();

      expect(find.text('Aún no hay notas. Crea una con «Nueva nota».'), findsOneWidget);
      await unmountWidget(tester);
    });

    testWidgets('un fallo de red se dice y Reintentar vuelve a leer', (tester) async {
      final transport = ScriptedTransport({
        'GET /v1/notes': okJson({'notes': [], 'next': null}),
      });
      transport.failNext('GET /v1/notes', const GatewayError(0, ''));

      await mountWidget(tester, AgendaNotesScreen(api: AgendaApi(transport)));
      await tester.pumpAndSettle();

      expect(find.text('No se pudo contactar con el equipo.'), findsOneWidget);
      await tester.tap(find.text('Reintentar'));
      await tester.pumpAndSettle();

      expect(find.text('No se pudo contactar con el equipo.'), findsNothing);
      expect(transport.calls, ['GET /v1/notes', 'GET /v1/notes']);
      await unmountWidget(tester);
    });
  });

  group('contacto', () {
    testWidgets('sin el permiso APPROVE, Borrar está apagado y dice por qué', (tester) async {
      final transport = ScriptedTransport({});
      transport.failNext(_probePath, const GatewayError(403, ''));

      await mountWidget(
        tester,
        AgendaContactScreen(api: AgendaApi(transport), contact: _ana()),
      );
      await tester.pumpAndSettle();

      expect(find.text(approveMissingText), findsOneWidget);
      final button = tester.widget<OutlinedButton>(
        find.widgetWithText(OutlinedButton, 'Borrar contacto'),
      );
      expect(button.onPressed, isNull);
      await unmountWidget(tester);
    });

    testWidgets('Cancelar en la confirmación no borra nada', (tester) async {
      final transport = ScriptedTransport({});
      transport.failNext(_probePath, const GatewayError(404, ''));

      await mountWidget(
        tester,
        AgendaContactScreen(api: AgendaApi(transport), contact: _ana()),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Borrar contacto'));
      await tester.pumpAndSettle();

      expect(find.text('¿Borrar este contacto?'), findsOneWidget);
      await tester.tap(find.text('Cancelar'));
      await tester.pumpAndSettle();

      expect(transport.calls, [_probePath]);
      await unmountWidget(tester);
    });

    testWidgets('con el permiso, Borrar se confirma y luego llama al host', (tester) async {
      final transport = ScriptedTransport({
        'POST /v1/contacts/ct_1/delete': okJson({}),
      });
      transport.failNext(_probePath, const GatewayError(404, ''));

      await mountWidget(
        tester,
        _pushedFrom(AgendaContactScreen(api: AgendaApi(transport), contact: _ana())),
      );
      await tester.tap(find.text('Abrir'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Borrar contacto'));
      await tester.pumpAndSettle();
      await tester.tap(find.widgetWithText(FilledButton, 'Borrar'));
      await tester.pumpAndSettle();

      expect(transport.calls, [_probePath, 'POST /v1/contacts/ct_1/delete']);
      expect(find.text('Abrir'), findsOneWidget);
      await unmountWidget(tester);
    });
  });

  group('importar', () {
    testWidgets('sin permiso del sistema dice que no se leyó nada', (tester) async {
      final transport = ScriptedTransport({});
      transport.failNext(_probePath, const GatewayError(404, ''));
      final phone = FakePhoneContacts(access: PhoneAccess.denied);

      await mountWidget(
        tester,
        AgendaImportScreen(api: AgendaApi(transport), phone: phone),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Permitir y leer los contactos'));
      await tester.pumpAndSettle();

      expect(
        find.text('No diste permiso, así que no se ha leído nada. Puedes intentarlo otra vez.'),
        findsOneWidget,
      );
      expect(find.text('Abrir ajustes de la app'), findsOneWidget);
      expect(phone.readCalls, 0);
      expect(transport.calls, [_probePath]);
      await unmountWidget(tester);
    });

    testWidgets('denegado para siempre ofrece abrir los ajustes de la app', (tester) async {
      final transport = ScriptedTransport({});
      transport.failNext(_probePath, const GatewayError(404, ''));
      final phone = FakePhoneContacts(access: PhoneAccess.deniedForever);

      await mountWidget(
        tester,
        AgendaImportScreen(api: AgendaApi(transport), phone: phone),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Permitir y leer los contactos'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Abrir ajustes de la app'));
      await tester.pumpAndSettle();

      expect(phone.settingsCalls, 1);
      expect(phone.readCalls, 0);
      await unmountWidget(tester);
    });

    testWidgets('sin APPROVE el botón está apagado y se explica', (tester) async {
      final transport = ScriptedTransport({});
      transport.failNext(_probePath, const GatewayError(403, ''));
      final phone = FakePhoneContacts();

      await mountWidget(
        tester,
        AgendaImportScreen(api: AgendaApi(transport), phone: phone),
      );
      await tester.pumpAndSettle();

      expect(find.text(approveMissingText), findsOneWidget);
      final button = tester.widget<FilledButton>(find.byType(FilledButton));
      expect(button.onPressed, isNull);
      expect(phone.readCalls, 0);
      await unmountWidget(tester);
    });
  });
}
