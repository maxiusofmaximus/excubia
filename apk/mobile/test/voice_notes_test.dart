import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/connection_failure.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/state/voice_errors.dart';
import 'package:excubia/core/state/voice_notes.dart';

import 'support/fake_chat_transport.dart';
import 'support/fake_voice_host.dart';
import 'support/fake_voice_recorder.dart';

/// Lets the transcript stream deliver what was added to it.
Future<void> settle() => Future<void>.delayed(Duration.zero);

HostError _refused(int status, String code, {int? retryAfter}) =>
    HostError(status, '{"detail":{"code":"$code"}}', retryAfter: retryAfter);

void main() {
  test(
    'soltar sube el audio como audio/ogg y entrega el texto sin enviarlo',
    () async {
      final host = FakeVoiceHost();
      final recorder = FakeVoiceRecorder(audio: [9, 8, 7]);
      final notes = VoiceNotes(host, recorder);
      addTearDown(notes.dispose);
      final texts = <String>[];
      final sub = notes.transcripts.listen(texts.add);
      addTearDown(sub.cancel);

      await notes.start();
      expect(notes.phase, VoicePhase.recording);
      await notes.release();
      await settle();

      expect(host.posts.single.target, '/v1/voice/transcribe');
      expect(host.posts.single.contentType, 'audio/ogg');
      expect(host.posts.single.body, [9, 8, 7]);
      expect(texts, ['hola mundo']);
      expect(
        host.calls,
        isEmpty,
        reason: 'the text is never sent as a message',
      );
      expect(notes.phase, VoicePhase.idle);
      expect(notes.notice, isNull);
      expect(File(recorder.lastPath!).existsSync(), isFalse);
    },
  );

  test('soltar dos veces a la vez transcribe una sola nota', () async {
    final host = FakeVoiceHost();
    final notes = VoiceNotes(host, FakeVoiceRecorder());
    addTearDown(notes.dispose);

    await notes.start();
    await Future.wait([notes.release(), notes.release()]);

    expect(host.posts, hasLength(1));
  });

  test('cancelar no hace ninguna petición y borra el temporal', () async {
    final host = FakeVoiceHost();
    final recorder = FakeVoiceRecorder();
    final notes = VoiceNotes(host, recorder);
    addTearDown(notes.dispose);

    await notes.start();
    await notes.cancel();

    expect(host.posts, isEmpty);
    expect(notes.phase, VoicePhase.idle);
    expect(File(recorder.lastPath!).existsSync(), isFalse);
  });

  group('refusals read as their own text', () {
    final cases = <(String, HostError, String)>[
      (
        'voice_unavailable',
        _refused(503, 'voice_unavailable'),
        'El PC no tiene la voz instalada o no está lista.',
      ),
      ('voice_busy', _refused(429, 'voice_busy', retryAfter: 2), voiceBusyText),
      (
        'audio_too_long',
        _refused(413, 'audio_too_long'),
        'La nota es demasiado larga.',
      ),
      (
        'invalid_audio',
        _refused(400, 'invalid_audio'),
        'No se pudo leer la nota. Vuelve a grabarla.',
      ),
      (
        'sin conexión',
        const HostError(0, 'Connection refused'),
        'Sin conexión con el PC.',
      ),
      (
        'plazo agotado',
        const HostError.transport(
          ConnectionFailure.timeout,
          'el equipo no respondió a tiempo',
        ),
        'El PC tardó demasiado en responder. Vuelve a intentarlo.',
      ),
      (
        'sin ruta al PC',
        const HostError.transport(ConnectionFailure.noRoute, 'refused'),
        'No llego al PC. ¿Está encendido y en la misma red? Si usas una VPN, compruébala.',
      ),
      (
        'PC con el servicio parado',
        const HostError(503, 'Service Unavailable'),
        'El PC responde, pero el servicio está parado. Arráncalo en el PC e inténtalo de nuevo.',
      ),
      (
        'respuesta 500',
        const HostError(500, 'internal error'),
        'No se pudo transcribir la nota. Vuelve a intentarlo.',
      ),
    ];
    for (final (name, error, text) in cases) {
      test(name, () async {
        final notes = VoiceNotes(
          FakeVoiceHost(refusal: error),
          FakeVoiceRecorder(),
        );
        addTearDown(notes.dispose);

        await notes.start();
        await notes.release();

        expect(notes.notice, text);
        expect(notes.phase, VoicePhase.idle);
      });
    }
  });

  test('voice_busy espera el Retry-After antes de la siguiente nota, sin reintentar', () async {
    final host = FakeVoiceHost(
      refusal: _refused(429, 'voice_busy', retryAfter: 30),
    );
    final recorder = FakeVoiceRecorder();
    final notes = VoiceNotes(host, recorder);
    addTearDown(notes.dispose);

    await notes.start();
    await notes.release();
    await notes.start();

    expect(host.posts, hasLength(1), reason: 'no automatic retry');
    expect(recorder.starts, 1);
    expect(notes.notice, voiceBusyText);
  });

  test('sin permiso no graba y lo dice', () async {
    final host = FakeVoiceHost();
    final recorder = FakeVoiceRecorder(allowed: false);
    final notes = VoiceNotes(host, recorder);
    addTearDown(notes.dispose);

    await notes.start();

    expect(recorder.starts, 0);
    expect(host.posts, isEmpty);
    expect(notes.notice, voicePermissionText);
    expect(notes.phase, VoicePhase.idle);
  });

  test('una nota sin voz avisa y no entrega texto', () async {
    final host = FakeVoiceHost(answer: '{"text":"   ","ms":4}');
    final notes = VoiceNotes(host, FakeVoiceRecorder());
    addTearDown(notes.dispose);
    final texts = <String>[];
    final sub = notes.transcripts.listen(texts.add);
    addTearDown(sub.cancel);

    await notes.start();
    await notes.release();
    await settle();

    expect(notes.notice, voiceEmptyText);
    expect(texts, isEmpty);
  });

  test('al llegar al límite de grabación corta y transcribe', () async {
    final host = FakeVoiceHost();
    final recorder = FakeVoiceRecorder();
    final notes = VoiceNotes(
      host,
      recorder,
      maxDuration: const Duration(milliseconds: 30),
      tick: const Duration(milliseconds: 10),
    );
    addTearDown(notes.dispose);
    final texts = <String>[];
    final sub = notes.transcripts.listen(texts.add);
    addTearDown(sub.cancel);

    await notes.start();
    while (host.posts.isEmpty) {
      await Future<void>.delayed(const Duration(milliseconds: 5));
    }
    await settle();

    expect(texts, ['hola mundo']);
    expect(notes.phase, VoicePhase.idle);
    expect(File(recorder.lastPath!).existsSync(), isFalse);
  });

  test('el límite por defecto es de 60 segundos', () {
    final notes = VoiceNotes(FakeChatTransport({}), FakeVoiceRecorder());
    addTearDown(notes.dispose);

    expect(notes.maxDuration, const Duration(seconds: 60));
  });

  test('en tocar, un toque empieza y el siguiente para y transcribe', () async {
    final host = FakeVoiceHost();
    final recorder = FakeVoiceRecorder();
    final notes = VoiceNotes(host, recorder);
    addTearDown(notes.dispose);
    final texts = <String>[];
    final sub = notes.transcripts.listen(texts.add);
    addTearDown(sub.cancel);

    await notes.toggle();
    expect(notes.phase, VoicePhase.recording);
    expect(host.posts, isEmpty);

    await notes.toggle();
    await settle();

    expect(host.posts, hasLength(1));
    expect(texts, ['hola mundo']);
    expect(notes.phase, VoicePhase.idle);
    expect(File(recorder.lastPath!).existsSync(), isFalse);
  });

  test(
    'en tocar, la nota sigue grabando sin soltar hasta el segundo toque',
    () async {
      final host = FakeVoiceHost();
      final notes = VoiceNotes(
        host,
        FakeVoiceRecorder(),
        tick: const Duration(milliseconds: 5),
      );
      addTearDown(notes.dispose);

      await notes.toggle();
      await Future<void>.delayed(const Duration(milliseconds: 30));

      expect(notes.phase, VoicePhase.recording);
      expect(host.posts, isEmpty);
      await notes.toggle();
    },
  );

  test(
    'en tocar, el límite de 60 s para solo y transcribe lo grabado',
    () async {
      final host = FakeVoiceHost();
      final notes = VoiceNotes(
        host,
        FakeVoiceRecorder(),
        maxDuration: const Duration(milliseconds: 30),
        tick: const Duration(milliseconds: 10),
      );
      addTearDown(notes.dispose);
      final texts = <String>[];
      final sub = notes.transcripts.listen(texts.add);
      addTearDown(sub.cancel);

      await notes.toggle();
      while (host.posts.isEmpty) {
        await Future<void>.delayed(const Duration(milliseconds: 5));
      }
      await settle();

      expect(texts, ['hola mundo']);
      expect(notes.phase, VoicePhase.idle);
    },
  );

  test('en tocar, cancelar descarta la nota sin pedir transcripción', () async {
    final host = FakeVoiceHost();
    final recorder = FakeVoiceRecorder();
    final notes = VoiceNotes(host, recorder);
    addTearDown(notes.dispose);

    await notes.toggle();
    await notes.cancel();

    expect(host.posts, isEmpty);
    expect(notes.phase, VoicePhase.idle);
    expect(File(recorder.lastPath!).existsSync(), isFalse);
  });
}
