import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/state/chat_attachment_tray.dart';
import 'package:excubia/core/state/voice_notes.dart';
import 'package:excubia/core/state/voice_send_settings.dart';
import 'package:excubia/ui/chat/chat_composer.dart';
import 'package:excubia/ui/theme.dart';

import 'support/fake_chat_transport.dart';
import 'support/fake_voice_host.dart';
import 'support/fake_voice_recorder.dart';
import 'support/memory_store.dart';

Widget _host(Widget child) => MaterialApp(
  theme: buildTheme(),
  home: Scaffold(body: child),
);

/// Lets the transcript stream deliver what was added to it.
Future<void> _settle(WidgetTester tester) => tester.runAsync(
  () => Future<void>.delayed(Duration.zero),
);

void main() {
  late TextEditingController text;
  late FocusNode focus;
  late ChatAttachmentTray tray;

  setUp(() {
    text = TextEditingController(text: 'borrador')
      ..selection = const TextSelection.collapsed(offset: 8);
    focus = FocusNode();
    tray = ChatAttachmentTray(FakeChatTransport({}));
  });

  tearDown(() {
    text.dispose();
    focus.dispose();
    tray.dispose();
  });

  ChatComposer composer({
    required VoiceNotes voice,
    required VoiceSendSettings voiceSend,
    required Future<bool> Function(String) onVoiceSend,
    bool busy = false,
  }) => ChatComposer(
    controller: text,
    focus: focus,
    enabled: true,
    sending: false,
    offline: false,
    busy: busy,
    canStop: busy,
    stopDenial: null,
    attachments: tray,
    voice: voice,
    voiceSend: voiceSend,
    onVoiceSend: onVoiceSend,
    onAttach: (_) {},
    onSend: () {},
    onStop: () {},
  );

  Future<VoiceSendSettings> settings(VoiceSendMode mode) async {
    final voiceSend = VoiceSendSettings(MemoryKeyValueStore());
    await voiceSend.choose(mode);
    return voiceSend;
  }

  Future<void> dictate(WidgetTester tester, VoiceNotes voice) async {
    await tester.runAsync(() async {
      await voice.start();
      await voice.release();
    });
    await _settle(tester);
    await tester.pump();
  }

  testWidgets('en modo cuadro el dictado se queda en el cuadro y no se envía', (
    tester,
  ) async {
    final voice = VoiceNotes(
      FakeVoiceHost(answer: '{"text":"hola","ms":1}'),
      FakeVoiceRecorder(),
    );
    addTearDown(voice.dispose);
    final sent = <String>[];

    await tester.pumpWidget(
      _host(
        composer(
          voice: voice,
          voiceSend: await settings(VoiceSendMode.draft),
          onVoiceSend: (t) async {
            sent.add(t);
            return true;
          },
        ),
      ),
    );
    await dictate(tester, voice);

    expect(sent, isEmpty);
    expect(text.text, 'borradorhola');
  });

  testWidgets('en modo automático el dictado se envía y el borrador se conserva', (
    tester,
  ) async {
    final voice = VoiceNotes(
      FakeVoiceHost(answer: '{"text":"hola","ms":1}'),
      FakeVoiceRecorder(),
    );
    addTearDown(voice.dispose);
    final sent = <String>[];

    await tester.pumpWidget(
      _host(
        composer(
          voice: voice,
          voiceSend: await settings(VoiceSendMode.auto),
          onVoiceSend: (t) async {
            sent.add(t);
            return true;
          },
        ),
      ),
    );
    await dictate(tester, voice);

    expect(sent, ['hola']);
    expect(text.text, 'borrador', reason: 'the draft stays as it was');
  });

  testWidgets('en modo automático, si el envío falla, el dictado vuelve al cursor', (
    tester,
  ) async {
    final voice = VoiceNotes(
      FakeVoiceHost(answer: '{"text":"querido","ms":1}'),
      FakeVoiceRecorder(),
    );
    addTearDown(voice.dispose);
    text.value = const TextEditingValue(
      text: 'hola  fin',
      selection: TextSelection.collapsed(offset: 5),
    );

    await tester.pumpWidget(
      _host(
        composer(
          voice: voice,
          voiceSend: await settings(VoiceSendMode.auto),
          onVoiceSend: (_) async => false,
        ),
      ),
    );
    await dictate(tester, voice);

    expect(text.text, 'hola querido fin');
    expect(focus.hasFocus, isTrue);
  });

  testWidgets('en modo automático una nota vacía no envía nada', (tester) async {
    final voice = VoiceNotes(
      FakeVoiceHost(answer: '{"text":"  ","ms":1}'),
      FakeVoiceRecorder(),
    );
    addTearDown(voice.dispose);
    final sent = <String>[];

    await tester.pumpWidget(
      _host(
        composer(
          voice: voice,
          voiceSend: await settings(VoiceSendMode.auto),
          onVoiceSend: (t) async {
            sent.add(t);
            return true;
          },
        ),
      ),
    );
    await dictate(tester, voice);

    expect(sent, isEmpty);
    expect(text.text, 'borrador');
    expect(voice.notice, isNotNull);
  });

  testWidgets('en modo automático el micro sigue junto a Detener mientras Alfred trabaja', (
    tester,
  ) async {
    final voice = VoiceNotes(FakeVoiceHost(), FakeVoiceRecorder());
    addTearDown(voice.dispose);
    text.clear();

    await tester.pumpWidget(
      _host(
        composer(
          voice: voice,
          voiceSend: await settings(VoiceSendMode.auto),
          onVoiceSend: (_) async => true,
          busy: true,
        ),
      ),
    );

    expect(find.byIcon(Icons.stop), findsOneWidget);
    expect(find.byIcon(Icons.mic), findsOneWidget);
  });

  testWidgets('en modo cuadro, mientras Alfred trabaja, el micro no aparece', (
    tester,
  ) async {
    final voice = VoiceNotes(FakeVoiceHost(), FakeVoiceRecorder());
    addTearDown(voice.dispose);
    text.clear();

    await tester.pumpWidget(
      _host(
        composer(
          voice: voice,
          voiceSend: await settings(VoiceSendMode.draft),
          onVoiceSend: (_) async => true,
          busy: true,
        ),
      ),
    );

    expect(find.byIcon(Icons.stop), findsOneWidget);
    expect(find.byIcon(Icons.mic), findsNothing);
  });

  testWidgets('el micro dice que envía al soltar solo en modo automático', (
    tester,
  ) async {
    final voice = VoiceNotes(FakeVoiceHost(), FakeVoiceRecorder());
    addTearDown(voice.dispose);
    text.clear();

    await tester.pumpWidget(
      _host(
        composer(
          voice: voice,
          voiceSend: await settings(VoiceSendMode.auto),
          onVoiceSend: (_) async => true,
        ),
      ),
    );
    expect(find.byTooltip('Mantén pulsado para grabar · envía al soltar'), findsOneWidget);

    await tester.pumpWidget(
      _host(
        composer(
          voice: voice,
          voiceSend: await settings(VoiceSendMode.draft),
          onVoiceSend: (_) async => true,
        ),
      ),
    );
    expect(find.byTooltip('Mantén pulsado para grabar'), findsOneWidget);
  });
}
