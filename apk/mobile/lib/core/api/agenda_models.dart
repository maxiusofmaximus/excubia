part of 'agenda_api.dart';

/// Where an address came from, as the host words it. A word this app does not
/// know has no tag, so [fromWire] answers `null`.
enum AgendaSource {
  app('app', 'creada aquí'),
  phone('phone', 'del teléfono'),
  alfred('alfred', 'Alfred'),
  userMessage('user_message', 'dictada');

  const AgendaSource(this.wire, this.label);

  final String wire;
  final String label;

  static AgendaSource? fromWire(String? wire) {
    for (final source in values) {
      if (source.wire == wire) return source;
    }
    return null;
  }
}

/// One page of a list and the cursor that reads the next one. `next` is `null`
/// on the last page. `truncated` is set when the host stopped early: the page was
/// full, or the host ran out of work for one read (calendar only).
class AgendaPage<T> {
  const AgendaPage({
    required this.items,
    required this.next,
    this.truncated = false,
  });

  final List<T> items;
  final String? next;
  final bool truncated;
}

/// A note as the list shows it: its subject and the start of its text.
class AgendaNoteRow {
  const AgendaNoteRow({
    required this.id,
    required this.subject,
    required this.excerpt,
    required this.updatedAt,
  });

  factory AgendaNoteRow.fromJson(Map<String, Object?> json) => AgendaNoteRow(
    id: jsonText(json, 'id'),
    subject: jsonOptionalText(json, 'subject'),
    excerpt: jsonOptionalText(json, 'excerpt') ?? '',
    updatedAt: _time(json, 'updated_at'),
  );

  final String id;
  final String? subject;
  final String excerpt;
  final DateTime updatedAt;
}

/// A note in full, for the editor.
class AgendaNote {
  const AgendaNote({
    required this.id,
    required this.subject,
    required this.body,
    required this.updatedAt,
  });

  factory AgendaNote.fromJson(Map<String, Object?> json) => AgendaNote(
    id: jsonText(json, 'id'),
    subject: jsonOptionalText(json, 'subject'),
    body: jsonOptionalText(json, 'body') ?? '',
    updatedAt: _time(json, 'updated_at'),
  );

  final String id;
  final String? subject;
  final String body;
  final DateTime updatedAt;
}

class AgendaAddress {
  const AgendaAddress({required this.address, required this.source});

  factory AgendaAddress.fromJson(Map<String, Object?> json) => AgendaAddress(
    address: jsonText(json, 'address'),
    source: AgendaSource.fromWire(jsonOptionalText(json, 'source')),
  );

  final String address;
  final AgendaSource? source;
}

class AgendaContact {
  const AgendaContact({
    required this.id,
    required this.name,
    required this.aliases,
    required this.addresses,
    required this.notes,
    required this.updatedAt,
  });

  factory AgendaContact.fromJson(Map<String, Object?> json) => AgendaContact(
    id: jsonText(json, 'id'),
    name: jsonText(json, 'name'),
    aliases: jsonStrings(json, 'aliases'),
    addresses: [
      for (final item in jsonList(json, 'addresses'))
        AgendaAddress.fromJson(jsonObject(item)),
    ],
    notes: jsonOptionalText(json, 'notes') ?? '',
    updatedAt: _time(json, 'updated_at'),
  );

  final String id;
  final String name;
  final List<String> aliases;
  final List<AgendaAddress> addresses;
  final String notes;
  final DateTime updatedAt;
}

/// One phone contact sent to the import: its name and its e-mails.
class AgendaImportEntry {
  const AgendaImportEntry({required this.name, required this.addresses});

  final String name;
  final List<String> addresses;
}

/// What the host did with an import: counts only, never a name or an address.
class AgendaImportReport {
  const AgendaImportReport({
    this.created = 0,
    this.merged = 0,
    this.skipped = 0,
    this.rejected = 0,
  });

  static const AgendaImportReport empty = AgendaImportReport();

  factory AgendaImportReport.fromJson(Map<String, Object?> json) =>
      AgendaImportReport(
        created: jsonInt(json, 'created'),
        merged: jsonInt(json, 'merged'),
        skipped: jsonInt(json, 'skipped'),
        rejected: jsonList(json, 'rejected').length,
      );

  final int created;
  final int merged;
  final int skipped;
  final int rejected;

  AgendaImportReport operator +(AgendaImportReport other) => AgendaImportReport(
    created: created + other.created,
    merged: merged + other.merged,
    skipped: skipped + other.skipped,
    rejected: rejected + other.rejected,
  );
}

DateTime _time(Map<String, Object?> json, String key) =>
    DateTime.parse(jsonText(json, key));
