import 'dart:convert';
import 'dart:typed_data';

import 'chat_transport.dart';
import 'connection_text.dart';
import 'host_client.dart';
import 'json_read.dart';

part 'agenda_failure_text.dart';
part 'agenda_models.dart';
part 'calendar_ics_models.dart';
part 'calendar_models.dart';
part 'ics_failure_text.dart';

/// The agenda routes: `/v1/notes`, `/v1/contacts` and the calendar (`/v1/calendar`,
/// `/v1/company/timezone`; contract `api.md`). Plain JSON over the signed transport,
/// as the mail routes are. Reads need `READ_STATE`, writes `RESPOND`, deletes and the
/// import `APPROVE`; the zone is read with `READ_STATE` and fixed with `APPROVE`.
class AgendaApi {
  const AgendaApi(this._transport);

  final ChatTransport _transport;

  Future<AgendaPage<AgendaNoteRow>> notes({
    String query = '',
    String? before,
  }) async {
    final body = await _read(
      _listPath('/v1/notes', query: query, cursorKey: 'before', cursor: before),
    );
    return AgendaPage(
      items: [
        for (final item in jsonList(body, 'notes'))
          AgendaNoteRow.fromJson(jsonObject(item)),
      ],
      next: jsonOptionalText(body, 'next'),
    );
  }

  Future<AgendaNote> note(String id) async {
    final body = await _read('/v1/notes/${Uri.encodeComponent(id)}');
    return AgendaNote.fromJson(jsonObject(body['note']));
  }

  /// [subject] is left out when empty; the host keeps no subject then.
  Future<AgendaNote> createNote({String? subject, required String body}) async {
    final json = await _write('/v1/notes', {'title': ?subject, 'body': body});
    return AgendaNote.fromJson(jsonObject(json['note']));
  }

  /// An empty [subject] clears the subject the note had.
  Future<AgendaNote> updateNote({
    required String id,
    required String subject,
    required String body,
  }) async {
    final json = await _write(_idPath('/v1/notes', id, 'update'), {
      'title': subject,
      'body': body,
    });
    return AgendaNote.fromJson(jsonObject(json['note']));
  }

  Future<void> deleteNote(String id) =>
      _write(_idPath('/v1/notes', id, 'delete'), const {});

  Future<AgendaPage<AgendaContact>> contacts({
    String query = '',
    String? after,
  }) async {
    final body = await _read(
      _listPath(
        '/v1/contacts',
        query: query,
        cursorKey: 'after',
        cursor: after,
      ),
    );
    return AgendaPage(
      items: [
        for (final item in jsonList(body, 'contacts'))
          AgendaContact.fromJson(jsonObject(item)),
      ],
      next: jsonOptionalText(body, 'next'),
    );
  }

  Future<AgendaContact> createContact({
    required String name,
    required List<String> aliases,
    required List<String> addresses,
    required String notes,
  }) async {
    final json = await _write('/v1/contacts', {
      'name': name,
      'aliases': aliases,
      'addresses': addresses,
      'notes': notes,
    });
    return AgendaContact.fromJson(jsonObject(json['contact']));
  }

  /// [addresses] is sent only when the person changed them: the host replaces
  /// the whole list and re-tags it as the app's, so an unchanged list is left out.
  Future<AgendaContact> updateContact({
    required String id,
    required String name,
    required List<String> aliases,
    required String notes,
    List<String>? addresses,
  }) async {
    final json = await _write(_idPath('/v1/contacts', id, 'update'), {
      'name': name,
      'aliases': aliases,
      'notes': notes,
      'addresses': ?addresses,
    });
    return AgendaContact.fromJson(jsonObject(json['contact']));
  }

  Future<void> deleteContact(String id) =>
      _write(_idPath('/v1/contacts', id, 'delete'), const {});

  /// One call of at most 500 entries; the caller splits longer lists.
  Future<AgendaImportReport> importContacts(
    List<AgendaImportEntry> entries,
  ) async {
    final json = await _write('/v1/contacts/import', {
      'contacts': [
        for (final entry in entries)
          {'name': entry.name, 'addresses': entry.addresses},
      ],
    });
    return AgendaImportReport.fromJson(json);
  }

  /// The events that start from [from] up to, not including, [to] (`YYYY-MM-DD`, in
  /// the company's zone). [after] is the `id` of the last event of the page before.
  Future<AgendaPage<CalendarEvent>> calendarEvents({
    required String from,
    required String to,
    String? after,
  }) async {
    final body = await _read(
      Uri(
        path: '/v1/calendar/events',
        queryParameters: {
          'from': from,
          'to': to,
          'limit': '$_pageSize',
          'after': ?after,
        },
      ).toString(),
    );
    return AgendaPage(
      items: [
        for (final item in jsonList(body, 'events'))
          CalendarEvent.fromJson(jsonObject(item)),
      ],
      next: jsonOptionalText(body, 'next'),
      truncated: body['truncated'] == true,
    );
  }

  /// One event, as stored: a series comes whole, with its own start.
  Future<CalendarEvent> calendarEvent(String id) async {
    final body = await _read('/v1/calendar/events/${Uri.encodeComponent(id)}');
    return CalendarEvent.fromJson(jsonObject(body['event']));
  }

  Future<void> createEvent(CalendarEventInput input) =>
      _write('/v1/calendar/events', input.toCreateJson());

  Future<void> updateEvent(String id, CalendarEventInput input) => _write(
    _idPath('/v1/calendar/events', id, 'update'),
    input.toUpdateJson(),
  );

  Future<void> deleteEvent(String id) =>
      _write(_idPath('/v1/calendar/events', id, 'delete'), const {});

  /// `null` until a device with `APPROVE` fixes the company's zone.
  Future<String?> companyTimezone() async =>
      jsonOptionalText(await _read('/v1/company/timezone'), 'timezone');

  Future<String> setCompanyTimezone(String zone) async => jsonText(
    await _write('/v1/company/timezone', {'timezone': zone}),
    'timezone',
  );

  /// The `.ics` of the events in [window] (`to` not included), or of all of them.
  Future<Uint8List> exportIcs([({String from, String to})? window]) async =>
      Uint8List.fromList(
        await _transport.callBytes('GET', _icsExportTarget(window)),
      );

  /// Sends the file as it is. One over [icsMaxBytes] is refused here, without a call.
  Future<IcsImportReport> importIcs(Uint8List file) async {
    if (file.length > icsMaxBytes) throw const HostError(413, '');
    final json = await _transport.postBytes(
      '/v1/calendar/import',
      file,
      'text/calendar',
    );
    return IcsImportReport.fromJson(jsonEnvelope(json));
  }

  /// Whether this device holds `APPROVE`. A delete of an id that does not exist
  /// is refused with `403` without the grant and answered `404` with it, so
  /// nothing is removed either way (the approvals route works the same way).
  Future<bool> canApprove() async {
    try {
      await _write(_idPath('/v1/contacts', _grantProbeId, 'delete'), const {});
      return true;
    } on HostError catch (error) {
      if (error.status == 404) return true;
      if (error.status == 403) return false;
      rethrow;
    }
  }

  Future<Map<String, Object?>> _read(String target) async =>
      jsonEnvelope(await _transport.call('GET', target));

  Future<Map<String, Object?>> _write(
    String target,
    Map<String, Object?> json,
  ) async => jsonEnvelope(
    await _transport.call('POST', target, utf8.encode(jsonEncode(json))),
  );

  static const String _grantProbeId = 'ct_grant_probe';

  static String _idPath(String route, String id, String action) =>
      '$route/${Uri.encodeComponent(id)}/$action';

  static String _icsExportTarget(({String from, String to})? window) =>
      window == null
      ? '/v1/calendar/export.ics'
      : Uri(
          path: '/v1/calendar/export.ics',
          queryParameters: {'from': window.from, 'to': window.to},
        ).toString();

  static String _listPath(
    String route, {
    required String query,
    required String cursorKey,
    String? cursor,
  }) => Uri(
    path: route,
    queryParameters: {
      if (query.isNotEmpty) 'q': query,
      'limit': '$_pageSize',
      cursorKey: ?cursor,
    },
  ).toString();
}

const int _pageSize = 20;
