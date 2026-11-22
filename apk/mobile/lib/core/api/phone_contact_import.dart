import 'agenda_api.dart';
import 'phone_contacts.dart';

/// The most entries one import call carries (the host's `import_limit`).
const int importBatchSize = 500;

/// One entry per phone contact that has an e-mail: the phone's name, or the first
/// address when the phone has none, and each address once. Rows with no address
/// are skipped, so the host never sees an empty entry.
List<AgendaImportEntry> importEntries(Iterable<PhoneEmailRow> rows) {
  final byContact = <String, List<PhoneEmailRow>>{};
  for (final row in rows) {
    if (row.address.trim().isEmpty) continue;
    byContact.putIfAbsent(row.contactId, () => []).add(row);
  }
  return [
    for (final group in byContact.values)
      AgendaImportEntry(
        name: _nameOf(group),
        addresses: {for (final row in group) row.address.trim()}.toList(),
      ),
  ];
}

/// The entries cut into calls of at most [size]; the last call may be shorter.
List<List<T>> inBatches<T>(List<T> items, {int size = importBatchSize}) => [
  for (var start = 0; start < items.length; start += size)
    items.skip(start).take(size).toList(),
];

String _nameOf(List<PhoneEmailRow> group) {
  for (final row in group) {
    final name = row.name?.trim();
    if (name != null && name.isNotEmpty) return name;
  }
  return group.first.address.trim();
}
