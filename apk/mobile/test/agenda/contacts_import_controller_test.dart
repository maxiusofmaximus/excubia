import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/agenda_api.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/phone_contacts.dart';
import 'package:excubia/core/state/contacts_import_controller.dart';

import '../support/fake_phone_contacts.dart';
import '../support/recording_transport.dart';

/// A phone whose contacts cannot be read, as after a broken provider.
class _UnreadablePhone implements PhoneContacts {
  @override
  Future<PhoneAccess> requestAccess() async => PhoneAccess.granted;

  @override
  Future<List<PhoneEmailRow>> readEmails() =>
      throw PlatformException(code: 'provider_gone');

  @override
  Future<void> openAppSettings() async {}
}

void main() {
  late RecordingTransport transport;

  setUp(() {
    transport = RecordingTransport();
    transport.answer = '{"ok":true,"created":1,"merged":0,"skipped":0,"rejected":[]}';
  });

  test('a denied permission reads nothing and sends nothing', () async {
    final phone = FakePhoneContacts(
      access: PhoneAccess.denied,
      rows: const [PhoneEmailRow(contactId: '1', name: 'Ana', address: 'ana@mail.es')],
    );
    final controller = ContactsImportController(phone: phone, api: AgendaApi(transport));

    await controller.run();

    expect(controller.phase, ContactsImportPhase.denied);
    expect(phone.readCalls, 0);
    expect(transport.targets, isEmpty);
  });

  test('a permanent denial is told apart from a plain one', () async {
    final phone = FakePhoneContacts(access: PhoneAccess.deniedForever);
    final controller = ContactsImportController(phone: phone, api: AgendaApi(transport));

    await controller.run();

    expect(controller.phase, ContactsImportPhase.deniedForever);
    expect(phone.readCalls, 0);
  });

  test('a granted import sends the batches and sums their counts', () async {
    final rows = [
      for (var i = 0; i < 501; i++)
        PhoneEmailRow(contactId: '$i', name: 'Contacto $i', address: 'c$i@mail.es'),
    ];
    final controller = ContactsImportController(
      phone: FakePhoneContacts(rows: rows),
      api: AgendaApi(transport),
    );

    await controller.run();

    expect(controller.phase, ContactsImportPhase.done);
    expect(transport.targets, ['POST /v1/contacts/import', 'POST /v1/contacts/import']);
    expect(controller.report?.created, 2);
    expect(controller.failure, isNull);
  });

  test('a phone that cannot be read gets its own failure text', () async {
    final controller = ContactsImportController(
      phone: _UnreadablePhone(),
      api: AgendaApi(transport),
    );

    await controller.run();

    expect(controller.phase, ContactsImportPhase.failed);
    expect(
      controller.failure,
      'No se pudieron leer los contactos del teléfono. Inténtalo de nuevo.',
    );
    expect(transport.targets, isEmpty);
  });

  test('a host refusal during the import is shown as the agenda failure', () async {
    transport.failure = const HostError(403, '');
    final controller = ContactsImportController(
      phone: FakePhoneContacts(
        rows: const [PhoneEmailRow(contactId: '1', name: 'Ana', address: 'ana@mail.es')],
      ),
      api: AgendaApi(transport),
    );

    await controller.run();

    expect(controller.phase, ContactsImportPhase.failed);
    expect(controller.failure, startsWith('Falta un permiso de este dispositivo'));
    expect(controller.report, isNull);
  });
}
