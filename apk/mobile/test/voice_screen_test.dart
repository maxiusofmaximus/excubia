import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/state/chat_attachment_tray.dart';
import 'package:excubia/core/state/voice_errors.dart';
import 'package:excubia/core/state/voice_notes.dart';
import 'package:excubia/core/state/voice_send_settings.dart';
import 'package:excubia/ui/chat/chat_composer.dart';
import 'package:excubia/ui/chat/voice_screen.dart';
import 'package:excubia/ui/theme.dart';

import 'support/fake_chat_transport.dart';
import 'support/fake_voice_host.dart';
import 'support/fake_voice_recorder.dart';

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

/// A page with one button that opens the voice screen as the chat does.
Widget _app(VoiceNotes voice) => MaterialApp(
  theme: buildTheme(),
  home: Scaffold(
    body: Builder(
      builder: (context) => TextButton(
        onPressed: () => Navigator.of(context).push(
          MaterialPageRoute<void>(
            fullscreenDialog: true,
            builder: (_) => VoiceScreen(voice: voice),
          ),
        ),
        child: const Text('abrir'),
      ),
    ),
  ),
);

Future<void> _open(WidgetTester tester) async {
  await tester.tap(find.text('abrir'));
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 400));
  await tester.pump();
}

/// The picture inside the orb: the face of the state the screen is in.
String _portraitOf(WidgetTester tester) =>
    (tester
            .widgetList<Image>(find.byType(Image))
            .map((image) => image.image)
            .whereType<AssetImage>()
            .where((image) => image.assetName.contains('portraits/'))
            .single)
        .assetName;

/// Waits until «Listo» has closed: its timer may run on the fake clock or in
/// real time, depending on where the transcript arrived, so both are advanced.
Future<void> _untilBack(WidgetTester tester) async {
  for (var i = 0; i < 300 && find.text('Listo').evaluate().isNotEmpty; i++) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 10)),
    );
    await tester.pump(const Duration(milliseconds: 20));
  }
  await tester.pump(const Duration(milliseconds: 400));
}

/// Cancels and waits for the file to go, then for the route to close.
Future<void> _cancel(WidgetTester tester, VoiceNotes voice) async {
  await tester.tap(find.text('Cancelar'));
  await _until(tester, () => voice.phase == VoicePhase.idle);
  await tester.pump(const Duration(milliseconds: 400));
}

Widget _host(Widget child) => MaterialApp(
  theme: buildTheme(),
  home: Scaffold(body: child),
);

void main() {
  testWidgets('al abrir empieza a grabar y dice «Te escucho»', (tester) async {
    final recorder = FakeVoiceRecorder();
    final voice = VoiceNotes(FakeVoiceHost(), recorder);
    addTearDown(voice.dispose);
    await tester.pumpWidget(_app(voice));

    await _open(tester);

    expect(recorder.starts, 1);
    expect(find.text('Te escucho'), findsOneWidget);
    expect(
      _portraitOf(tester),
      'assets/agents_v3/portraits/alfred-neutral-512.webp',
    );
    expect(
      find.text('Habla con normalidad; pulsa Terminar cuando acabes.'),
      findsOneWidget,
    );
    // A level moves the ring without a single error.
    recorder.emitLevel(0.8);
    await tester.pump();
    expect(tester.takeException(), isNull);

    await _cancel(tester, voice);
  });

  testWidgets(
    'Terminar transcribe, dice «Listo» con el texto y vuelve al chat',
    (tester) async {
      final host = FakeVoiceHost(answer: '{"text":"hola mundo","ms":1}');
      final voice = VoiceNotes(host, FakeVoiceRecorder());
      addTearDown(voice.dispose);
      await tester.pumpWidget(_app(voice));
      await _open(tester);

      await tester.tap(find.text('Terminar'));
      await tester.pump();
      expect(find.text('Transcribiendo…'), findsOneWidget);
      expect(
        _portraitOf(tester),
        'assets/agents_v3/portraits/alfred-thinking-512.webp',
      );

      await _until(tester, () => voice.phase == VoicePhase.idle);
      expect(find.text('Listo'), findsOneWidget);
      expect(
        _portraitOf(tester),
        'assets/agents_v3/portraits/alfred-done-512.webp',
      );
      expect(find.text('hola mundo'), findsOneWidget);
      expect(host.posts.single.target, '/v1/voice/transcribe');

      // «Listo» stays a moment, then the chat is back.
      await _untilBack(tester);
      expect(find.text('Listo'), findsNothing);
      expect(find.text('abrir'), findsOneWidget);
    },
  );

  testWidgets('un error llano se dice y Reintentar vuelve a grabar', (
    tester,
  ) async {
    final recorder = FakeVoiceRecorder();
    final host = FakeVoiceHost(refusal: const HostError.transport(null, ''));
    final voice = VoiceNotes(host, recorder);
    addTearDown(voice.dispose);
    await tester.pumpWidget(_app(voice));
    await _open(tester);

    await tester.tap(find.text('Terminar'));
    await _until(tester, () => voice.phase == VoicePhase.idle);

    expect(find.text('Sin conexión con el PC.'), findsOneWidget);
    expect(find.text('Reintentar'), findsOneWidget);

    await tester.tap(find.text('Reintentar'));
    await tester.pump();
    expect(recorder.starts, 2);
    expect(find.text('Te escucho'), findsOneWidget);

    await _cancel(tester, voice);
  });

  testWidgets('sin permiso de micrófono dice por qué y ofrece Ajustes', (
    tester,
  ) async {
    final voice = VoiceNotes(
      FakeVoiceHost(),
      FakeVoiceRecorder(allowed: false),
    );
    addTearDown(voice.dispose);
    await tester.pumpWidget(_app(voice));

    await _open(tester);

    expect(find.text(voicePermissionText), findsOneWidget);
    expect(find.text('Abrir ajustes'), findsOneWidget);
    expect(find.text('Reintentar'), findsOneWidget);

    await tester.tap(find.text('Cancelar'));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('abrir'), findsOneWidget);
  });

  testWidgets('Cancelar descarta la grabación sin pedir transcripción', (
    tester,
  ) async {
    final host = FakeVoiceHost();
    final voice = VoiceNotes(host, FakeVoiceRecorder());
    addTearDown(voice.dispose);
    await tester.pumpWidget(_app(voice));
    await _open(tester);

    await _cancel(tester, voice);

    expect(host.posts, isEmpty);
    expect(find.text('Te escucho'), findsNothing);
    expect(find.text('abrir'), findsOneWidget);
  });

  testWidgets('un toque corto en el micrófono abre la pantalla de voz', (
    tester,
  ) async {
    final recorder = FakeVoiceRecorder();
    final voice = VoiceNotes(FakeVoiceHost(), recorder)..markExplained();
    addTearDown(voice.dispose);
    final text = TextEditingController();
    addTearDown(text.dispose);
    final focus = FocusNode();
    addTearDown(focus.dispose);
    final tray = ChatAttachmentTray(FakeChatTransport({}));
    addTearDown(tray.dispose);

    await tester.pumpWidget(
      _host(
        ChatComposer(
          controller: text,
          focus: focus,
          enabled: true,
          sending: false,
          offline: false,
          busy: false,
          canStop: false,
          stopDenial: null,
          attachments: tray,
          voice: voice,
          voiceSend: VoiceSendSettings(),
          onVoiceSend: (_) async => false,
          onAttach: (_) {},
          onSend: () {},
          onStop: () {},
        ),
      ),
    );

    await tester.tap(find.byIcon(Icons.mic));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    await tester.pump();

    expect(find.text('Te escucho'), findsOneWidget);
    expect(recorder.starts, 1);

    await _cancel(tester, voice);
  });

  testWidgets(
    'mantener pulsado sigue grabando como antes, sin abrir la pantalla',
    (tester) async {
      final recorder = FakeVoiceRecorder();
      final host = FakeVoiceHost(answer: '{"text":"hola mundo","ms":1}');
      final voice = VoiceNotes(host, recorder)..markExplained();
      addTearDown(voice.dispose);
      final text = TextEditingController();
      addTearDown(text.dispose);
      final focus = FocusNode();
      addTearDown(focus.dispose);
      final tray = ChatAttachmentTray(FakeChatTransport({}));
      addTearDown(tray.dispose);

      await tester.pumpWidget(
        _host(
          ChatComposer(
            controller: text,
            focus: focus,
            enabled: true,
            sending: false,
            offline: false,
            busy: false,
            canStop: false,
            stopDenial: null,
            attachments: tray,
            voice: voice,
            voiceSend: VoiceSendSettings(),
            onVoiceSend: (_) async => false,
            onAttach: (_) {},
            onSend: () {},
            onStop: () {},
          ),
        ),
      );

      final gesture = await tester.startGesture(
        tester.getCenter(find.byIcon(Icons.mic)),
      );
      await tester.pump(const Duration(milliseconds: 700));
      expect(recorder.starts, 1);
      expect(find.text('Te escucho'), findsNothing);

      await gesture.up();
      await _until(tester, () => voice.phase == VoicePhase.idle);
      expect(host.posts, hasLength(1));
      expect(text.text, 'hola mundo');
    },
  );
}
