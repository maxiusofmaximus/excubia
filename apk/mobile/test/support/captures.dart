import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

/// The folder named by [variable], or null when the test should only render.
String? evidenceDir(String variable) => Platform.environment[variable];

/// flutter_test draws text in Ahem and icons as boxes. The app asks for the
/// Windows sans ('Segoe UI'), 'monospace' and the Material icon font; loading
/// the real faces under those names makes the captures readable.
Future<void> loadReadableFonts() async {
  const icons =
      r'C:\Users\Max\flutter\bin\cache\artifacts\material_fonts\materialicons-regular.otf';
  for (final family in ['Segoe UI', 'Roboto']) {
    await (FontLoader(
      family,
    )..addFont(_font(r'C:\Windows\Fonts\segoeui.ttf'))).load();
  }
  await (FontLoader(
    'monospace',
  )..addFont(_font(r'C:\Windows\Fonts\consola.ttf'))).load();
  await (FontLoader('MaterialIcons')..addFont(_font(icons))).load();
}

Future<ByteData> _font(String path) async =>
    ByteData.sublistView(await File(path).readAsBytes());

/// Writes the [key] boundary as a PNG, or does nothing without a folder.
Future<void> saveCapture(
  WidgetTester tester,
  GlobalKey key,
  String? dir,
  String name,
) async {
  if (dir == null) return;
  final boundary =
      key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
  final bytes = await tester.runAsync(() async {
    final image = await boundary.toImage(pixelRatio: 2);
    final data = await image.toByteData(format: ui.ImageByteFormat.png);
    return data!.buffer.asUint8List();
  });
  await tester.runAsync(() => File('$dir/$name.png').writeAsBytes(bytes!));
}
