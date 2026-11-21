import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/ui/theme.dart';
import 'package:excubia/ui/widgets/comic/comic_tab_bar.dart';

void main() {
  testWidgets('en la barra inferior de un Scaffold mide poco, no toda la pantalla', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Scaffold(
          body: const Text('contenido'),
          bottomNavigationBar: ComicTabBar(
            selected: 0,
            onSelected: (_) {},
            tabs: const [
              ComicTab(
                label: 'Chat',
                icon: Icon(Icons.chat_outlined),
                selectedIcon: Icon(Icons.forum),
              ),
              ComicTab(
                label: 'Equipo',
                icon: Icon(Icons.groups_outlined),
                selectedIcon: Icon(Icons.groups),
              ),
            ],
          ),
        ),
      ),
    );
    final bar = tester.getSize(find.byType(ComicTabBar)).height;
    expect(bar, lessThan(120));
    expect(bar, greaterThanOrEqualTo(48));
    expect(find.text('contenido'), findsOneWidget);
  });
}
