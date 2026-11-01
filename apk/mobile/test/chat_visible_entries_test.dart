import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';
import 'package:herdr_notify/ui/chat/chat_conversation_view.dart';

ChatEntry _message(int seq) => ChatEntry(
  seq: seq,
  kind: 'message',
  suggestions: const [],
  body: 'Hola',
  preview: 'Hola',
  createdAt: '2026-10-25T10:00:00.000Z',
  local: false,
);

ChatEntry _draft(int seq, String draftId) => ChatEntry(
  seq: seq,
  kind: 'mail_draft',
  suggestions: const [],
  body: '',
  preview: 'Borrador',
  mailDraft: MailDraftCard(
    account: 'trabajo',
    draftId: draftId,
    to: const ['bea@ejemplo.org'],
    subject: 'Presupuesto',
  ),
  createdAt: '2026-10-25T10:00:00.000Z',
  local: false,
);

void main() {
  test('keeps only the newest review card of each draft', () {
    final entries = visibleEntries([
      _draft(3, 'dft_1'),
      _message(4),
      _draft(5, 'dft_1'),
      _draft(6, 'dft_2'),
    ]);

    expect(entries.map((entry) => entry.seq), [4, 5, 6]);
  });
}
