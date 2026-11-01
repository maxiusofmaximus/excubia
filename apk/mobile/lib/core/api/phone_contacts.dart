import 'package:flutter/services.dart';

/// The system's answer to the contacts question. `deniedForever` is the answer
/// after which the system stops asking: only the phone's settings can change it.
enum PhoneAccess { granted, denied, deniedForever }

/// One e-mail of one phone contact. It lives in memory only, while it is sent.
class PhoneEmailRow {
  const PhoneEmailRow({
    required this.contactId,
    required this.name,
    required this.address,
  });

  final String contactId;
  final String? name;
  final String address;
}

/// What the import needs from the phone. Tests put a fake in its place.
abstract interface class PhoneContacts {
  /// Asks the system. It answers at once when the permission was granted before.
  Future<PhoneAccess> requestAccess();

  /// The e-mail rows of every contact. Only after [PhoneAccess.granted].
  Future<List<PhoneEmailRow>> readEmails();

  Future<void> openAppSettings();
}

/// The phone's contacts through `dev.herdrnotify/contacts` (HerdrContactsPlugin.kt).
class ChannelPhoneContacts implements PhoneContacts {
  const ChannelPhoneContacts();

  static const MethodChannel _channel = MethodChannel(
    'dev.herdrnotify/contacts',
  );

  @override
  Future<PhoneAccess> requestAccess() async =>
      switch (await _channel.invokeMethod<String>('requestAccess')) {
        'granted' => PhoneAccess.granted,
        'deniedForever' => PhoneAccess.deniedForever,
        _ => PhoneAccess.denied,
      };

  @override
  Future<List<PhoneEmailRow>> readEmails() async {
    final rows =
        await _channel.invokeListMethod<Map<Object?, Object?>>('readEmails') ??
        const [];
    return [
      for (final row in rows)
        PhoneEmailRow(
          contactId: '${row['contactId']}',
          name: _text(row['name']),
          address: _text(row['address']) ?? '',
        ),
    ];
  }

  @override
  Future<void> openAppSettings() =>
      _channel.invokeMethod<void>('openAppSettings');

  static String? _text(Object? value) => value is String ? value : null;
}
