import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/app_info.dart';

void main() {
  // The About line must show the version the build actually carries.
  test('appVersion is the version declared in pubspec.yaml', () {
    final pubspec = File('pubspec.yaml').readAsStringSync();
    expect(pubspec, contains('version: $appVersion'));
  });
}
