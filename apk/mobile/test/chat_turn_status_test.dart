import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/src/rust/api/chat/model.dart';
import 'package:excubia/src/rust/api/chat/reducer.dart';
import 'package:excubia/src/rust/frb_generated.dart';
import 'package:excubia/ui/chat/chat_bubbles.dart';
import 'package:excubia/ui/chat/chat_progress.dart';
import 'package:excubia/ui/chat/chat_screen.dart';

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  group('the step line under the own message', () {
    test('every host turn state has its text', () {
      expect(turnLine(turnState: 'queued', latestOwn: true)?.text, 'Enviado');
      expect(
        turnLine(
          turnState: 'running',
          latestOwn: true,
          runningAgent: 'Alfred',
        )?.text,
        'Alfred lo está atendiendo',
      );
      expect(
        turnLine(turnState: 'running', latestOwn: true)?.text,
        'Atendiendo',
      );
      expect(
        turnLine(turnState: 'completed', latestOwn: true)?.text,
        'Respondido',
      );
      expect(turnLine(turnState: 'failed', latestOwn: true)?.text, 'Falló');
      expect(
        turnLine(turnState: 'interrupted', latestOwn: true)?.text,
        'Detenido',
      );
    });

    test('a message without a turn state shows no line', () {
      expect(turnLine(turnState: null, latestOwn: true), isNull);
    });

    test('an older own message shows only a failure', () {
      expect(turnLine(turnState: 'completed', latestOwn: false), isNull);
      expect(turnLine(turnState: 'running', latestOwn: false), isNull);
      expect(turnLine(turnState: 'failed', latestOwn: false)?.text, 'Falló');
    });
  });

  test('«Ir al final» counts Alfred’s messages after the seq seen at the end', () {
    final reducer = newChatReducer();
    reducer.acceptTimeline(
      json:
          '{"ok":true,"entries":['
          '{"seq":1,"kind":"message","message_id":"m1","author_kind":"user"},'
          '{"seq":2,"kind":"message","message_id":"m2","author_kind":"agent"},'
          '{"seq":3,"kind":"message","message_id":"m3","author_kind":"user"},'
          '{"seq":4,"kind":"message","message_id":"m4","author_kind":"agent"}]}',
    );

    final entries = reducer.snapshot().entries;
    expect(unseenAgentMessages(entries, 2), 1);
    expect(unseenAgentMessages(entries, 0), 2);
    expect(unseenAgentMessages(entries, 4), 0);
  });

  test('a step is spoken at most once every five seconds', () {
    final voice = StepAnnouncer();
    final start = DateTime(2026, 10, 26, 10);
    expect(voice.label('Alfred · A', start), 'Alfred · A');
    expect(
      voice.label('Alfred · B', start.add(const Duration(seconds: 2))),
      'Alfred · A',
    );
    expect(
      voice.label('Alfred · B', start.add(const Duration(seconds: 5))),
      'Alfred · B',
    );
    expect(voice.label(null, start.add(const Duration(seconds: 6))), isNull);
  });

  test('a host that predates the turn fields still decodes', () {
    final conversation = parseChatConversation(
      json: '{"ok":true,"conversation":{"id":"cnv_1","created_at":"a"}}',
    );
    expect(conversation?.activeRuns, 0);

    final frame = parseChatEvent(
      json:
          '{"event":"chat.message","conversation_id":"cnv_1",'
          '"change":"updated","message_id":"m1","state":"failed"}',
    );
    expect(frame?.turnState, isNull);
    expect(frame?.state, 'failed');
  });
}
