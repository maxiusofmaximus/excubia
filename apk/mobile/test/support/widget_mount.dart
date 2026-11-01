import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/ui/theme.dart';

/// A host answer of the form `{"ok": true, ...}`.
String okJson(Map<String, Object?> fields) =>
    jsonEncode({'ok': true, ...fields});

/// Mounts [home] in the app theme on a view of [size], and restores the view.
Future<void> mountWidget(
  WidgetTester tester,
  Widget home, {
  Size size = const Size(800, 3200),
}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(MaterialApp(theme: buildTheme(), home: home));
  await tester.pump();
}

/// Leaving the tree disposes the controllers and stops their timers.
Future<void> unmountWidget(WidgetTester tester) async {
  await tester.pumpWidget(const SizedBox());
}
