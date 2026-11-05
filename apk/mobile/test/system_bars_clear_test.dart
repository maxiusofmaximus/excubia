import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/ui/app.dart';
import 'package:herdr_notify/ui/theme.dart';

void main() {
  testWidgets('the last row of a list ends above the bar of buttons', (tester) async {
    const bar = 48.0;
    Widget bar48(BuildContext c, Widget? child) => MediaQuery(
      data: MediaQuery.of(c).copyWith(padding: const EdgeInsets.only(bottom: bar)),
      child: child!,
    );
    Widget app(TransitionBuilder? clear) => MaterialApp(
      theme: buildTheme(HerdrPalettes.pergamino),
      builder: (c, child) =>
          bar48(c, clear == null ? child : Builder(builder: (c) => clear(c, child))),
      home: Scaffold(
        body: ListView(
          padding: const EdgeInsets.all(16),
          children: [for (var i = 0; i < 40; i++) Text('fila $i'), const Text('fin')],
        ),
      ),
    );
    await tester.binding.setSurfaceSize(const Size(400, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(app(withSystemBarsClear));
    await tester.drag(find.byType(ListView), const Offset(0, -5000));
    await tester.pumpAndSettle();
    expect(tester.getBottomLeft(find.text('fin')).dy, lessThanOrEqualTo(800 - bar));
  });
}
