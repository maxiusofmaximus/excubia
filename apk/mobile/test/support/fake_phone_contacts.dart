import 'package:herdr_notify/core/api/phone_contacts.dart';

/// A phone with the answer and the rows the test sets. It counts the calls, so a
/// test can show that a denial never reads the contacts.
class FakePhoneContacts implements PhoneContacts {
  FakePhoneContacts({
    this.access = PhoneAccess.granted,
    this.rows = const [],
  });

  PhoneAccess access;
  final List<PhoneEmailRow> rows;
  int readCalls = 0;
  int settingsCalls = 0;

  @override
  Future<PhoneAccess> requestAccess() async => access;

  @override
  Future<List<PhoneEmailRow>> readEmails() async {
    readCalls++;
    return rows;
  }

  @override
  Future<void> openAppSettings() async {
    settingsCalls++;
  }
}
