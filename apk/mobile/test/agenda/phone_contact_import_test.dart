import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/phone_contact_import.dart';
import 'package:excubia/core/api/phone_contacts.dart';

PhoneEmailRow _row(String id, String? name, String address) =>
    PhoneEmailRow(contactId: id, name: name, address: address);

void main() {
  group('importEntries', () {
    test('groups the e-mails of one contact and skips rows without one', () {
      final entries = importEntries([
        _row('1', 'Ana', 'ana@mail.es'),
        _row('2', 'Luis', ''),
        _row('1', 'Ana', 'ana.trabajo@mail.es'),
        _row('3', 'Sin correo', '   '),
      ]);

      expect(entries.map((entry) => entry.name), ['Ana']);
      expect(entries.single.addresses, ['ana@mail.es', 'ana.trabajo@mail.es']);
    });

    test('names a contact after its first name, or its first address', () {
      final entries = importEntries([
        _row('1', null, 'anonimo@mail.es'),
        _row('2', '  ', 'otra@mail.es'),
        _row('3', ' Marta ', 'marta@mail.es'),
      ]);

      expect(entries.map((entry) => entry.name), [
        'anonimo@mail.es',
        'otra@mail.es',
        'Marta',
      ]);
    });

    test('sends each address of a contact once', () {
      final entries = importEntries([
        _row('1', 'Ana', 'ana@mail.es'),
        _row('1', 'Ana', ' ana@mail.es '),
      ]);

      expect(entries.single.addresses, ['ana@mail.es']);
    });
  });

  group('inBatches', () {
    test('cuts the entries into calls of at most 500', () {
      final batches = inBatches(List<int>.generate(1001, (index) => index));

      expect(batches.map((batch) => batch.length), [500, 500, 1]);
      expect(batches.last, [1000]);
    });

    test('keeps one short call for a short list and none for an empty one', () {
      expect(inBatches([1, 2, 3]), [
        [1, 2, 3],
      ]);
      expect(inBatches(<int>[]), isEmpty);
    });

    test('exactly 500 entries make one full call', () {
      final batches = inBatches(List<int>.generate(importBatchSize, (i) => i));

      expect(batches.length, 1);
      expect(batches.single.length, 500);
    });
  });
}
