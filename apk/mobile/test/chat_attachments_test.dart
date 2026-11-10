import 'dart:async';
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/chat_attachment_source.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/state/chat_attachment_errors.dart';
import 'package:excubia/core/state/chat_attachment_images.dart';
import 'package:excubia/core/state/chat_attachment_tray.dart';
import 'package:excubia/core/state/chat_controller.dart';
import 'package:excubia/src/rust/frb_generated.dart';

import 'support/fake_attachments.dart';

const _deleteAtt1 = 'DELETE /v1/chat/conversations/cnv_1/attachments/att_1';

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  group('the tray', () {
    test('holds four files at most', () async {
      final tray = ChatAttachmentTray(
        UploadTransport(chatRoutes()),
        pause: noPause,
      );
      addTearDown(tray.dispose);

      await tray.add('cnv_1', [for (var i = 0; i < 5; i++) png('$i.png')]);
      await settle(tray);

      expect(tray.items, hasLength(4));
      expect(tray.canAdd, isFalse);
    });

    test('Enviar waits for every file: ready only once all are', () async {
      final transport = UploadTransport(chatRoutes())
        ..holdCreate = Completer<void>();
      final tray = ChatAttachmentTray(transport, pause: noPause);
      addTearDown(tray.dispose);

      await tray.add('cnv_1', [png()]);
      expect(tray.allReady, isFalse);

      transport.holdCreate!.complete();
      await settle(tray);
      expect(tray.allReady, isTrue);
      expect(tray.readyFiles.single.id, 'att_1');
    });

    test('an empty tray is ready; a refused file blocks sending', () async {
      final tray = ChatAttachmentTray(
        UploadTransport(chatRoutes()),
        pause: noPause,
      );
      addTearDown(tray.dispose);
      expect(tray.allReady, isTrue);

      await tray.add('cnv_1', [
        MemorySource('nota.gif', List.filled(12, 0x47)),
      ]);
      await settle(tray);

      expect(tray.allReady, isFalse);
      expect(tray.items.single.status, DraftStatus.failed);
      expect(tray.items.single.error, 'Solo PNG, JPEG o WebP');
      expect(tray.readyFiles, isEmpty);
    });

    test('removing an uploaded file deletes it on the host', () async {
      final transport = UploadTransport(chatRoutes());
      final tray = ChatAttachmentTray(transport, pause: noPause);
      addTearDown(tray.dispose);
      await tray.add('cnv_1', [png()]);
      await settle(tray);

      await tray.remove(tray.items.single);

      expect(tray.items, isEmpty);
      expect(transport.calls, contains(_deleteAtt1));
    });

    test(
      'an offset mismatch resumes the upload from the host offset',
      () async {
        final transport = UploadTransport(chatRoutes())
          ..chunkRefusals.add(
            const HostError(
              409,
              '{"detail":{"code":"offset_mismatch","message":"x","next_offset":4}}',
            ),
          );
        final tray = ChatAttachmentTray(transport, pause: noPause);
        addTearDown(tray.dispose);

        await tray.add('cnv_1', [png()]);
        await settle(tray);

        expect(tray.allReady, isTrue);
        expect(transport.chunkOffsets, [0, 4]);
      },
    );
  });

  group('the messages', () {
    test('each refusal code reads as its line; an unknown one is generic', () {
      expect(attachmentErrorText('unsupported_image'), 'Solo PNG, JPEG o WebP');
      expect(
        attachmentErrorText('dimensions_too_large'),
        'La imagen es demasiado grande',
      );
      expect(
        attachmentErrorText('host_quota'),
        'Se llenó el espacio de adjuntos del equipo',
      );
      expect(
        attachmentErrorText('weird'),
        'No se pudo subir el archivo. Vuelve a intentarlo',
      );
    });

    test('a 422 that names unknown_attachment is found in its body', () {
      expect(
        namesUnknownAttachment(
          const HostError(422, '{"detail":{"code":"unknown_attachment"}}'),
        ),
        isTrue,
      );
      expect(
        namesUnknownAttachment(
          const HostError(422, '{"detail":{"code":"other"}}'),
        ),
        isFalse,
      );
      expect(namesUnknownAttachment(StateError('x')), isFalse);
    });

    test('the first bytes decide the type, not the name', () {
      expect(isSupportedImage([0x89, 0x50, 0x4E, 0x47]), isTrue);
      expect(isSupportedImage([0xFF, 0xD8, 0xFF, 0xE0]), isTrue);
      expect(
        isSupportedImage([
          ...'RIFF'.codeUnits,
          0,
          0,
          0,
          0,
          ...'WEBP'.codeUnits,
        ]),
        isTrue,
      );
      expect(
        isSupportedImage([
          ...'RIFF'.codeUnits,
          0,
          0,
          0,
          0,
          ...'WAVE'.codeUnits,
        ]),
        isFalse,
      );
      expect(isSupportedImage('GIF89a'.codeUnits), isFalse);
    });
  });

  group('the stored images', () {
    test('a failed load is cached too, so it is not asked again', () async {
      var asked = 0;
      final images = ChatAttachmentImages((_) async {
        asked++;
        throw const HostError(502, 'down');
      });

      await expectLater(images.load('att_1'), throwsA(isA<HostError>()));
      await expectLater(images.load('att_1'), throwsA(isA<HostError>()));
      expect(asked, 1);
    });

    test('the least recently used picture goes first', () async {
      var asked = 0;
      final images = ChatAttachmentImages((_) async {
        asked++;
        return [1];
      }, capacity: 2);

      await images.load('a');
      await images.load('b');
      await images.load('a');
      await images.load('c'); // evicts b
      await images.load('a'); // still cached
      await images.load('b'); // asked again

      expect(asked, 4);
    });
  });

  group('a message with files', () {
    test('names its ready files and may carry no text', () async {
      final transport = UploadTransport(chatRoutes());
      final controller = ChatController(transport, dummyTriage());
      addTearDown(controller.dispose);
      await controller.start();
      await controller.attachments.add('cnv_1', [png()]);
      await settle(controller.attachments);

      await controller.send('', files: controller.attachments.readyFiles);

      final sent = transport.calls.indexOf(
        'POST /v1/chat/conversations/cnv_1/messages',
      );
      final body = jsonDecode(utf8.decode(transport.bodies[sent])) as Map;
      expect(body['text'], '');
      expect(body['attachments'], ['att_1']);
      // Accepted: the timeline now carries the files, so the local list goes.
      expect(controller.attachmentsOfSend(body['client_id'] as String), isNull);
      expect(controller.attachments.items, isEmpty);
    });

    test('a message from before attachments has none', () async {
      final controller = ChatController(
        UploadTransport(chatRoutes()),
        dummyTriage(),
      );
      addTearDown(controller.dispose);
      await controller.start();

      expect(controller.entries.single.attachments, isNull);
    });
  });
}
