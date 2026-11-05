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

/// Mounts [home] at a phone's size (390 dp wide, 3x) with the system text scale
/// at [textScale], as the device shows it.
Future<void> mountAtPhone(
  WidgetTester tester,
  Widget home, {
  double textScale = 1,
}) async {
  tester.view.physicalSize = const Size(390 * 3, 844 * 3);
  tester.view.devicePixelRatio = 3;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      theme: buildTheme(),
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(
          context,
        ).copyWith(textScaler: TextScaler.linear(textScale)),
        child: child!,
      ),
      home: home,
    ),
  );
  await tester.pump();
}

/// Leaving the tree disposes the controllers and stops their timers.
Future<void> unmountWidget(WidgetTester tester) async {
  await tester.pumpWidget(const SizedBox());
}
