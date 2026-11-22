import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/app_info.dart';

void main() {
  test('the version shown in Ajustes is the one in pubspec.yaml', () {
    final line = File('pubspec.yaml').readAsLinesSync().firstWhere((l) => l.startsWith('version:'));
    expect(appVersion, line.substring('version:'.length).trim());
  });
}
