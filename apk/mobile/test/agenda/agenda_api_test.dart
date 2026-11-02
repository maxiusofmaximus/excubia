import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/agenda_api.dart';
import 'package:herdr_notify/core/api/host_client.dart';

import '../support/recording_transport.dart';

const String _noteRow =
    '{"id":"nt_1","subject":"Cliente","excerpt":"presupuesto","updated_at":"2026-10-29T09:00:00.000Z"}';

void main() {
  late RecordingTransport transport;
  late AgendaApi api;

  setUp(() {
    transport = RecordingTransport();
    api = AgendaApi(transport);
  });

  group('notes', () {
    test('reads the first page with the limit and no query', () async {
      transport.answer = '{"ok":true,"notes":[$_noteRow],"next":"nt_1"}';

      final page = await api.notes();

      expect(transport.targets.single, 'GET /v1/notes?limit=20');
      expect(page.items.single.subject, 'Cliente');
      expect(page.next, 'nt_1');
    });

    test('sends the query and the cursor of the page before', () async {
      transport.answer = '{"ok":true,"notes":[],"next":null}';

      final page = await api.notes(query: 'cliente 1', before: 'nt_9');

      expect(transport.targets.single, 'GET /v1/notes?q=cliente+1&limit=20&before=nt_9');
      expect(page.next, isNull);
    });
  });

  test('a new note leaves out the subject when it is empty', () async {
    transport.answer = '{"ok":true,"note":{"id":"nt_2","subject":null,"body":"hola","updated_at":"2026-10-29T09:00:00.000Z"}}';

    final note = await api.createNote(body: 'hola');

    expect(transport.targets.single, 'POST /v1/notes');
    expect(transport.bodies.single, {'body': 'hola'});
    expect(note.body, 'hola');
  });

  test('an empty subject is sent as an empty title, which clears it', () async {
    transport.answer = '{"ok":true,"note":{"id":"nt_1","subject":null,"body":"x","updated_at":"2026-10-29T09:00:00.000Z"}}';

    await api.updateNote(id: 'nt_1', subject: '', body: 'x');

    expect(transport.targets.single, 'POST /v1/notes/nt_1/update');
    expect(transport.bodies.single, {'title': '', 'body': 'x'});
  });

  test('a contact update sends the addresses only when they changed', () async {
    transport.answer = '{"ok":true,"contact":{"id":"ct_1","name":"Ana","aliases":[],"addresses":[],"notes":"","updated_at":"2026-10-29T09:00:00.000Z"}}';

    await api.updateContact(id: 'ct_1', name: 'Ana', aliases: const [], notes: '');
    expect(transport.bodies.last.containsKey('addresses'), isFalse);

    await api.updateContact(
      id: 'ct_1',
      name: 'Ana',
      aliases: const [],
      notes: '',
      addresses: const ['ana@mail.es'],
    );
    expect(transport.bodies.last['addresses'], ['ana@mail.es']);
  });

  test('the import sends the entries and reads the counts only', () async {
    transport.answer = '{"ok":true,"created":2,"merged":1,"skipped":0,"rejected":[{"index":3,"code":"no_address"}]}';

    final report = await api.importContacts(const [
      AgendaImportEntry(name: 'Ana', addresses: ['ana@mail.es']),
    ]);

    expect(transport.targets.single, 'POST /v1/contacts/import');
    expect(transport.bodies.single, {
      'contacts': [
        {'name': 'Ana', 'addresses': ['ana@mail.es']},
      ],
    });
    expect(report.created, 2);
    expect(report.merged, 1);
    expect(report.rejected, 1);
  });

  group('canApprove', () {
    test('a 404 on the probe means the grant is there', () async {
      transport.failure = const HostError(404, '{"detail":{"code":"not_found"}}');

      expect(await api.canApprove(), isTrue);
      expect(transport.targets.single, 'POST /v1/contacts/ct_grant_probe/delete');
    });

    test('a 403 on the probe means the grant is missing', () async {
      transport.failure = const HostError(403, '{"detail":{"code":"forbidden"}}');

      expect(await api.canApprove(), isFalse);
    });

    test('any other answer is an error, not a guess', () async {
      transport.failure = const HostError(500, '');

      await expectLater(api.canApprove(), throwsA(isA<HostError>()));
    });
  });

  group('agendaFailureText', () {
    String textOf(int status, Map<String, Object?> detail) =>
        agendaFailureText(HostError(status, jsonEncode({'detail': detail})));

    test('an address owned by another contact names it', () {
      expect(
        textOf(409, {
          'code': 'address_taken',
          'message': 'the address already belongs to the contact Luis Pérez',
        }),
        'Esa dirección ya es de «Luis Pérez».',
      );
    });

    test('an address owner that the message does not name gets the plain text', () {
      expect(
        textOf(409, {'code': 'address_taken', 'message': 'otra cosa'}),
        'Esa dirección ya es de otro contacto.',
      );
    });

    test('an invalid argument names the field that is wrong', () {
      expect(
        textOf(422, {'code': 'invalid_arguments', 'field': 'aliases'}),
        'Hay más de 10 alias, o alguno es demasiado largo.',
      );
      expect(
        textOf(422, {'code': 'invalid_arguments', 'field': 'name'}),
        'El nombre es obligatorio y tiene 120 caracteres como máximo.',
      );
    });

    test('the limits and the missing permission have their own text', () {
      expect(textOf(409, {'code': 'contact_limit'}), 'Ya hay 1000 contactos. Borra alguno para crear otro.');
      expect(textOf(409, {'code': 'note_limit'}), 'Ya hay 2000 notas. Borra alguna para crear otra.');
      expect(
        agendaFailureText(const HostError(403, '')),
        startsWith('Falta un permiso de este dispositivo'),
      );
    });

    test('a repeat rule the app cannot send or the host cannot read has its own text', () {
      expect(
        textOf(422, {'code': 'unsupported_recurrence'}),
        'Esa repetición todavía no está soportada.',
      );
      expect(textOf(422, {'code': 'invalid_recurrence'}), 'La repetición no es válida.');
      expect(
        textOf(422, {'code': 'invalid_field', 'field': 'recurrence'}),
        'La repetición no es válida.',
      );
    });

    test('a code the app does not know gets the generic text, never the host words', () {
      expect(
        textOf(500, {'code': 'algo_raro', 'message': 'secreto interno'}),
        'El equipo no pudo completar la acción. Inténtalo de nuevo.',
      );
    });

    test('a failure that is not a host error gets the generic text', () {
      expect(agendaFailureText(StateError('x')), 'El equipo no pudo completar la acción. Inténtalo de nuevo.');
    });
  });
}
