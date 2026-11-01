import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../api/agenda_api.dart';
import '../api/phone_contact_import.dart';
import '../api/phone_contacts.dart';

/// Where the import stands. `denied` can be asked again; after `deniedForever`
/// only the phone's settings can change the answer, so the screen offers them.
enum ContactsImportPhase { idle, running, done, denied, deniedForever, failed }

/// Asks for the phone's contacts, reads their e-mails and sends them to the PC
/// in batches. Nothing is kept: the rows live only while they are sent.
class ContactsImportController extends ChangeNotifier {
  ContactsImportController({required this._phone, required this._api});

  final PhoneContacts _phone;
  final AgendaApi _api;
  ContactsImportPhase _phase = ContactsImportPhase.idle;
  AgendaImportReport? _report;
  String? _failure;
  bool _disposed = false;

  ContactsImportPhase get phase => _phase;

  /// The totals of the last finished import, summed over its batches.
  AgendaImportReport? get report => _report;
  String? get failure => _failure;

  /// One tap of the person: asks, reads and sends. The host keeps what is already
  /// there, so running it again only adds what is new.
  Future<void> run() async {
    if (_phase == ContactsImportPhase.running) return;
    _report = null;
    _failure = null;
    _set(ContactsImportPhase.running);
    try {
      final access = await _phone.requestAccess();
      if (access != PhoneAccess.granted) {
        _set(
          access == PhoneAccess.denied
              ? ContactsImportPhase.denied
              : ContactsImportPhase.deniedForever,
        );
        return;
      }
      final entries = importEntries(await _phone.readEmails());
      var total = AgendaImportReport.empty;
      for (final batch in inBatches(entries)) {
        total += await _api.importContacts(batch);
      }
      _report = total;
      _set(ContactsImportPhase.done);
    } catch (error) {
      _failure = error is PlatformException
          ? 'No se pudieron leer los contactos del teléfono. Inténtalo de nuevo.'
          : agendaFailureText(error);
      _set(ContactsImportPhase.failed);
    }
  }

  void _set(ContactsImportPhase phase) {
    _phase = phase;
    if (!_disposed) notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
