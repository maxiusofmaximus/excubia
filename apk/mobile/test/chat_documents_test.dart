import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/chat_attachment_source.dart';
import 'package:excubia/core/api/chat_document_files.dart';
import 'package:excubia/core/api/chat_document_previews.dart';
import 'package:excubia/core/api/chat_documents.dart';
import 'package:excubia/core/state/chat_attachment_errors.dart';
import 'package:excubia/core/state/chat_document_opener.dart';
import 'package:excubia/src/rust/api/chat/attach_model.dart';
import 'package:excubia/ui/mail/mail_facts.dart' show sizeLabel;

import 'support/fake_attachments.dart';
import 'support/fake_documents.dart';
import 'support/fake_preview_transport.dart';

ChatAttachment _file(String name, {String mime = 'application/pdf', int size = 12}) =>
    ChatAttachment(
      id: 'att_1',
      name: name,
      mime: mime,
      size: BigInt.from(size),
      w: 0,
      h: 0,
    );

void main() {
  group('tipos de documento', () {
    test('la lista blanca es la del host, sin macros ni formatos viejos', () {
      expect(
        chatDocumentExtensions,
        unorderedEquals(['xlsx', 'docx', 'pptx', 'pdf', 'csv', 'txt', 'md', 'json']),
      );
      for (final name in ['plantilla.xlsm', 'carta.docm', 'hoja.xls', 'carta.doc']) {
        expect(documentKindOf(name), isNull, reason: name);
      }
    });

    test('el tipo sale de la extensión, sin distinguir mayúsculas', () {
      expect(documentKindOf('Informe.XLSX'), ChatDocKind.sheet);
      expect(documentKindOf('datos.csv'), ChatDocKind.sheet);
      expect(documentKindOf('Carta.docx'), ChatDocKind.word);
      expect(documentKindOf('deck.pptx'), ChatDocKind.slides);
      expect(documentKindOf('manual.pdf'), ChatDocKind.pdf);
      expect(documentKindOf('nota.md'), ChatDocKind.text);
      expect(documentKindOf('leeme.txt'), ChatDocKind.text);
      expect(documentKindOf('datos.json'), ChatDocKind.text);
      expect(documentKindOf('captura.png'), isNull);
      expect(documentKindOf('sin_extension'), isNull);
    });

    test('solo xlsx, docx y pdf tienen vista previa', () {
      for (final name in ['Informe.XLSX', 'carta.docx', 'manual.pdf']) {
        expect(isPreviewableDocument(name), isTrue, reason: name);
      }
      for (final name in ['datos.csv', 'deck.pptx', 'leeme.txt', 'nota.md', 'datos.json']) {
        expect(isPreviewableDocument(name), isFalse, reason: name);
      }
    });

    test('cada tipo tiene su etiqueta en español', () {
      expect(ChatDocKind.sheet.label, 'Hoja de cálculo');
      expect(ChatDocKind.word.label, 'Documento');
      expect(ChatDocKind.slides.label, 'Presentación');
      expect(ChatDocKind.pdf.label, 'PDF');
      expect(ChatDocKind.text.label, 'Texto');
    });

    test('el tamaño se lee con coma decimal', () {
      expect(sizeLabel(3565158), '3,4 MB');
    });

    test('un adjunto es documento por su nombre; una imagen no', () {
      expect(isDocumentFile(_file('Informe.xlsx')), isTrue);
      expect(isDocumentFile(_file('captura.webp', mime: 'image/webp')), isFalse);
    });
  });

  group('nombre en la caché', () {
    test('quita lo que el sistema de archivos no admite', () {
      expect(cacheFileName('a/b:c*?.xlsx'), 'a_b_c__.xlsx');
      expect(cacheFileName('../../etc/passwd'), isNot(contains('/')));
    });

    test('un nombre que es solo puntos o vacío toma uno genérico', () {
      expect(cacheFileName('..'), 'documento');
      expect(cacheFileName('   '), 'documento');
    });

    test('un nombre largo conserva su extensión', () {
      final name = cacheFileName('${'x' * 200}.pdf');
      expect(name.length, 100);
      expect(name, endsWith('.pdf'));
    });
  });

  group('rechazo antes de subir', () {
    test('un documento de 10 MiB exactos pasa sin leer su contenido', () async {
      final source = MemorySource('datos.xlsx', List.filled(maxAttachmentBytes, 0));
      expect(await refusalOf(source), isNull);
    });

    test('un documento de más de 10 MiB se rechaza', () async {
      final source = MemorySource(
        'datos.xlsx',
        List.filled(maxAttachmentBytes + 1, 0),
      );
      expect(await refusalOf(source), 'document_too_large');
    });

    test('un tipo fuera de la lista se rechaza', () async {
      expect(await refusalOf(MemorySource('programa.exe', [1, 2, 3])), 'unsupported_type');
    });

    test('una imagen que no es PNG, JPEG ni WebP se rechaza como imagen', () async {
      final gif = MemorySource('nota.gif', List.filled(12, 0x47));
      expect(await refusalOf(gif), 'unsupported_image');
    });

    test('un PNG válido pasa', () async {
      expect(await refusalOf(png()), isNull);
    });

    test('los textos de cada rechazo están en español', () {
      expect(attachmentErrorText('unsupported_type'), 'Ese tipo de archivo no se admite');
      expect(attachmentErrorText('unsupported_image'), 'Solo PNG, JPEG o WebP');
      expect(attachmentErrorText('document_too_large'), 'El archivo supera los 10 MB');
    });
  });

  group('abrir y guardar', () {
    late Directory root;
    late FakeDocumentFiles files;

    setUp(() async {
      root = await Directory.systemTemp.createTemp('chat_docs');
      files = FakeDocumentFiles();
    });

    tearDown(() async {
      if (await root.exists()) await root.delete(recursive: true);
    });

    ChatDocumentOpener opener({List<int> bytes = const [1, 2, 3]}) =>
        ChatDocumentOpener(
          fetch: (_) async => bytes,
          files: files,
          root: () async => root,
          previews: ChatDocumentPreviews(
            FakePreviewTransport(),
            conversation: () => null,
          ),
        );

    test('escribe el archivo con su nombre y lo abre con su tipo', () async {
      final result = await opener().open(_file('Informe final.xlsx'));

      expect(result, isA<DocumentOpened>());
      expect(files.openedPath, endsWith('/Informe final.xlsx'));
      expect(files.openedMime, 'application/pdf');
      expect(await File(files.openedPath!).readAsBytes(), [1, 2, 3]);
    });

    test('sin app que lo abra, da los bytes para guardarlos', () async {
      files.outcome = OpenOutcome.noApp;
      final result = await opener().open(_file('manual.pdf'));

      expect(result, isA<DocumentNoApp>());
      expect((result as DocumentNoApp).bytes, [1, 2, 3]);
    });

    test('guardar pasa el nombre, el tipo y los bytes al selector', () async {
      final saved = await opener().save(
        _file('manual.pdf'),
        Uint8List.fromList([9, 8]),
      );

      expect(saved, isTrue);
      expect(files.saves.single.name, 'manual.pdf');
      expect(files.saves.single.mime, 'application/pdf');
      expect(files.saves.single.bytes, [9, 8]);
    });

    test('al iniciar borra lo abierto hace más de 24 h y deja lo reciente', () async {
      final old = File('${root.path}/viejo.pdf')..writeAsBytesSync([1]);
      final fresh = File('${root.path}/nuevo.pdf')..writeAsBytesSync([1]);
      final now = DateTime(2026, 11, 6, 12);
      old.setLastModifiedSync(now.subtract(const Duration(hours: 25)));
      fresh.setLastModifiedSync(now.subtract(const Duration(hours: 2)));

      await pruneOpenedDocuments(root, now: now);

      expect(old.existsSync(), isFalse);
      expect(fresh.existsSync(), isTrue);
    });
  });
}
