import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/state/chat_attachment_tray.dart';
import 'package:herdr_notify/core/state/voice_notes.dart';
import 'package:herdr_notify/core/state/voice_send_settings.dart';
import 'package:herdr_notify/ui/chat/chat_composer.dart';
import 'package:herdr_notify/ui/theme.dart';

import 'support/fake_chat_transport.dart';
import 'support/fake_voice_host.dart';
import 'support/fake_voice_recorder.dart';
import 'support/memory_store.dart';

/// A note reads and deletes its real file, which runs outside the fake clock:
/// waits in real time, letting the frames draw, until [done] holds.
Future<void> _until(WidgetTester tester, bool Function() done) async {
  for (var i = 0; i < 100 && !done(); i++) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 10)),
    );
    await tester.pump();
  }
  await tester.pump();
}

Widget _host(Widget child) => MaterialApp(
  theme: buildTheme(),
  home: Scaffold(body: child),
);

Future<VoiceSendSettings> _settings(VoiceSendMode send) async {
  final voiceSend = VoiceSendSettings(MemoryKeyValueStore());
  await voiceSend.choose(send);
  await voiceSend.chooseMic(VoiceMicMode.toggle);
  return voiceSend;
}

Widget _composer({
  required TextEditingController text,
  required FocusNode focus,
  required VoiceNotes voice,
  required VoiceSendSettings voiceSend,
  Future<bool> Function(String text)? onVoiceSend,
}) => _host(
  ChatComposer(
    controller: text,
    focus: focus,
    enabled: true,
    sending: false,
    offline: false,
    busy: false,
    canStop: false,
    stopDenial: null,
    attachments: ChatAttachmentTray(FakeChatTransport({})),
    voice: voice,
    voiceSend: voiceSend,
    onVoiceSend: onVoiceSend ?? (_) async => false,
    onAttach: () {},
    onSend: () {},
    onStop: () {},
  ),
);

void main() {
  testWidgets(
    'un toque empieza, el botón muestra que toca para parar y el segundo transcribe',
    (tester) async {
      final text = TextEditingController();
      addTearDown(text.dispose);
      final focus = FocusNode();
      addTearDown(focus.dispose);
      final host = FakeVoiceHost();
      final voice = VoiceNotes(host, FakeVoiceRecorder())..markExplained();
      addTearDown(voice.dispose);

      await tester.pumpWidget(
        _composer(
          text: text,
          focus: focus,
          voice: voice,
          voiceSend: await _settings(VoiceSendMode.draft),
        ),
      );

      await tester.tap(find.byIcon(Icons.mic));
      await _until(tester, () => voice.phase == VoicePhase.recording);

      expect(find.byIcon(Icons.stop), findsOneWidget);
      expect(find.textContaining('Toca para parar'), findsOneWidget);
      expect(host.posts, isEmpty);

      await tester.tap(find.byIcon(Icons.stop));
      await _until(tester, () => voice.phase == VoicePhase.idle);

      expect(host.posts, hasLength(1));
      expect(text.text, 'hola mundo');
      expect(find.byIcon(Icons.arrow_upward), findsOneWidget);
    },
  );

  testWidgets(
    'con envío automático, cada nota en tocar se envía una sola vez',
    (tester) async {
      final text = TextEditingController();
      addTearDown(text.dispose);
      final focus = FocusNode();
      addTearDown(focus.dispose);
      final voice = VoiceNotes(FakeVoiceHost(), FakeVoiceRecorder())
        ..markExplained();
      addTearDown(voice.dispose);
      final sent = <String>[];

      await tester.pumpWidget(
        _composer(
          text: text,
          focus: focus,
          voice: voice,
          voiceSend: await _settings(VoiceSendMode.auto),
          onVoiceSend: (t) async {
            sent.add(t);
            return true;
          },
        ),
      );

      for (var note = 0; note < 2; note++) {
        await tester.tap(find.byIcon(Icons.mic));
        await _until(tester, () => voice.phase == VoicePhase.recording);
        await tester.tap(find.byIcon(Icons.stop));
        await _until(tester, () => voice.phase == VoicePhase.idle);
      }

      expect(sent, ['hola mundo', 'hola mundo']);
      expect(text.text, isEmpty);
    },
  );

  testWidgets('la x cancela la grabación en tocar y no pide transcripción', (
    tester,
  ) async {
    final text = TextEditingController();
    addTearDown(text.dispose);
    final focus = FocusNode();
    addTearDown(focus.dispose);
    final host = FakeVoiceHost();
    final voice = VoiceNotes(host, FakeVoiceRecorder())..markExplained();
    addTearDown(voice.dispose);

    await tester.pumpWidget(
      _composer(
        text: text,
        focus: focus,
        voice: voice,
        voiceSend: await _settings(VoiceSendMode.draft),
      ),
    );

    await tester.tap(find.byIcon(Icons.mic));
    await _until(tester, () => voice.phase == VoicePhase.recording);
    await tester.tap(find.byIcon(Icons.close));
    await _until(tester, () => voice.phase == VoicePhase.idle);

    expect(host.posts, isEmpty);
    expect(text.text, isEmpty);
  });

  testWidgets('pasar a segundo plano cancela la grabación sin enviarla', (
    tester,
  ) async {
    final text = TextEditingController();
    addTearDown(text.dispose);
    final focus = FocusNode();
    addTearDown(focus.dispose);
    final host = FakeVoiceHost();
    final voice = VoiceNotes(host, FakeVoiceRecorder())..markExplained();
    addTearDown(voice.dispose);
    addTearDown(
      () => tester.binding.handleAppLifecycleStateChanged(
        AppLifecycleState.resumed,
      ),
    );

    await tester.pumpWidget(
      _composer(
        text: text,
        focus: focus,
        voice: voice,
        voiceSend: await _settings(VoiceSendMode.draft),
      ),
    );

    await tester.tap(find.byIcon(Icons.mic));
    await _until(tester, () => voice.phase == VoicePhase.recording);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.paused);
    await _until(tester, () => voice.phase == VoicePhase.idle);

    expect(host.posts, isEmpty);
    expect(text.text, isEmpty);
  });

  testWidgets('el primer toque explica qué se graba antes de empezar', (
    tester,
  ) async {
    final text = TextEditingController();
    addTearDown(text.dispose);
    final focus = FocusNode();
    addTearDown(focus.dispose);
    final voice = VoiceNotes(FakeVoiceHost(), FakeVoiceRecorder());
    addTearDown(voice.dispose);

    await tester.pumpWidget(
      _composer(
        text: text,
        focus: focus,
        voice: voice,
        voiceSend: await _settings(VoiceSendMode.draft),
      ),
    );

    await tester.tap(find.byIcon(Icons.mic));
    await tester.pump();
    expect(find.textContaining('Al tocar el micrófono'), findsOneWidget);
    expect(voice.phase, VoicePhase.idle);

    await tester.tap(find.text('Continuar'));
    await _until(tester, () => voice.phase == VoicePhase.recording);

    expect(voice.phase, VoicePhase.recording);
  });
}
