import '../api/mail_api.dart';
import 'mail_draft_controller.dart';

/// The draft controllers, by account and draft id. A controller outlives its
/// sheet, so a send keeps being watched after the sheet closes, and its final
/// result shows the next time the draft opens. A finished flow that left nothing
/// scheduled asks again from the same controller (see `canRequestSend`).
class MailDrafts {
  MailDrafts(this._mail, {this.tick = const Duration(seconds: 1), this.clock});

  final MailApi _mail;
  final Duration tick;
  final DateTime Function()? clock;
  final Map<String, MailDraftController> _open = {};

  /// The controller of this draft, created on first use.
  MailDraftController draftFor(String account, String draftId) =>
      _open.putIfAbsent(
        '$account/$draftId',
        () => MailDraftController(
          _mail,
          account: account,
          draftId: draftId,
          tick: tick,
          clock: clock,
        ),
      );

  void dispose() {
    for (final draft in _open.values) {
      draft.dispose();
    }
    _open.clear();
  }
}
