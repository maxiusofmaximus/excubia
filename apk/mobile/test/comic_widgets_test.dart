import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/ui/theme.dart';
import 'package:excubia/ui/widgets/comic/comic_button.dart';
import 'package:excubia/ui/widgets/comic/comic_portrait.dart';
import 'package:excubia/ui/widgets/comic/comic_switch.dart';
import 'package:excubia/ui/widgets/comic/comic_tag.dart';
import 'package:excubia/ui/widgets/agent_portrait_card.dart';

Widget _host(Widget child) => MaterialApp(
  theme: buildTheme(),
  home: Scaffold(body: Center(child: child)),
);

void main() {
  group('ComicButton', () {
    testWidgets('se ve su etiqueta y mide al menos 48 dp', (tester) async {
      await tester.pumpWidget(
        _host(ComicButton(label: 'Permitir', onPressed: () {})),
      );
      expect(find.text('Permitir'), findsOneWidget);
      expect(
        tester.getSize(find.byType(ComicButton)).height,
        greaterThanOrEqualTo(48),
      );
    });

    testWidgets('es un botón con su etiqueta semántica', (tester) async {
      await tester.pumpWidget(
        _host(ComicButton(label: 'Denegar', onPressed: () {})),
      );
      expect(find.bySemanticsLabel('Denegar'), findsOneWidget);
    });

    testWidgets('pulsarlo llama a onPressed', (tester) async {
      var taps = 0;
      await tester.pumpWidget(
        _host(ComicButton(label: 'Enviar', onPressed: () => taps++)),
      );
      await tester.tap(find.text('Enviar'));
      expect(taps, 1);
    });

    testWidgets('sin onPressed aparece deshabilitado', (tester) async {
      await tester.pumpWidget(
        _host(const ComicButton(label: 'Enviar', onPressed: null)),
      );
      expect(
        tester.getSemantics(find.bySemanticsLabel('Enviar')),
        matchesSemantics(
          label: 'Enviar',
          isButton: true,
          hasEnabledState: true,
          isEnabled: false,
        ),
      );
    });
  });

  group('ComicTag', () {
    testWidgets(
      'en mayúsculas por defecto, con la etiqueta semántica original',
      (tester) async {
        await tester.pumpWidget(_host(const ComicTag(label: 'Nueva')));
        expect(find.text('NUEVA'), findsOneWidget);
        expect(find.bySemanticsLabel('Nueva'), findsOneWidget);
      },
    );

    testWidgets('caps: false conserva el texto tal cual', (tester) async {
      await tester.pumpWidget(
        _host(const ComicTag(label: '¡Tu OK!', caps: false)),
      );
      expect(find.text('¡Tu OK!'), findsOneWidget);
    });
  });

  group('ComicSwitch', () {
    testWidgets('al pulsar avisa del valor contrario', (tester) async {
      bool? seen;
      await tester.pumpWidget(
        _host(
          ComicSwitch(
            value: false,
            onChanged: (v) => seen = v,
            semanticLabel: 'Avisos',
          ),
        ),
      );
      await tester.tap(find.byType(ComicSwitch));
      expect(seen, isTrue);
    });

    testWidgets('sin onChanged aparece deshabilitado', (tester) async {
      await tester.pumpWidget(
        _host(
          const ComicSwitch(
            value: true,
            onChanged: null,
            semanticLabel: 'Avisos',
          ),
        ),
      );
      expect(
        tester.getSemantics(find.bySemanticsLabel('Avisos')),
        matchesSemantics(
          label: 'Avisos',
          hasToggledState: true,
          isToggled: true,
          hasEnabledState: true,
          isEnabled: false,
        ),
      );
    });

    testWidgets('expone el estado activado y su etiqueta', (tester) async {
      await tester.pumpWidget(
        _host(
          ComicSwitch(value: true, onChanged: (_) {}, semanticLabel: 'Avisos'),
        ),
      );
      expect(
        tester.getSemantics(find.bySemanticsLabel('Avisos')),
        matchesSemantics(
          label: 'Avisos',
          hasToggledState: true,
          isToggled: true,
          hasEnabledState: true,
          isEnabled: true,
        ),
      );
    });
  });

  group('retrato de Alfred', () {
    testWidgets('el retrato comic de 220 dp se monta sin fallar', (
      tester,
    ) async {
      await tester.pumpWidget(
        _host(
          const SizedBox(
            width: 220,
            child: AgentPortraitCard(id: 'alfred', name: 'Alfred'),
          ),
        ),
      );
      expect(find.text('Alfred'), findsOneWidget);
      expect(find.byType(ComicPortrait), findsOneWidget);
      expect(tester.takeException(), isNull);
    });
  });
}
