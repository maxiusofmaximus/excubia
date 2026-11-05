import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/ui/theme.dart';

import 'support/contrast.dart';

/// Every item of [items] against every surface, as `name sobre surface: ratio`
/// for each pair below [minimum]. An empty list means the palette passes.
List<String> _belowMinimum(
  Map<String, Color> items,
  Map<String, Color> surfaces,
  double minimum,
) => [
  for (final item in items.entries)
    for (final surface in surfaces.entries)
      if (contrastRatio(item.value, surface.value) < minimum)
        '${item.key} sobre ${surface.key}: '
            '${contrastRatio(item.value, surface.value).toStringAsFixed(2)}:1',
];

void main() {
  for (final palette in HerdrPalettes.all) {
    group('paleta ${palette.name}', () {
      final c = palette.colors;
      final surfaces = {
        'bg': c.bg,
        'surface': c.surface,
        'raised': c.raised,
        'surface3': c.surface3,
        'accentSoft': c.accentSoft,
      };

      test('texto normal cumple 4,5:1 sobre cada superficie', () {
        final texts = {
          'textPrimary': c.textPrimary,
          'textSecondary': c.textSecondary,
          'textMuted': c.textMuted,
          'accentText': c.accentText,
          'statusBlocked': c.statusBlocked,
          'statusWorking': c.statusWorking,
          'statusDone': c.statusDone,
          'statusIdle': c.statusIdle,
          'statusUnknown': c.statusUnknown,
        };
        expect(_belowMinimum(texts, surfaces, 4.5), isEmpty);
      });

      test('texto sobre relleno cumple 4,5:1', () {
        final fills = {
          'onAccent sobre accent': (c.onAccent, c.accent),
          'onAttention sobre statusBlocked': (c.onAttention, c.statusBlocked),
        };
        final failures = [
          for (final fill in fills.entries)
            if (contrastRatio(fill.value.$1, fill.value.$2) < 4.5)
              '${fill.key}: ${contrastRatio(fill.value.$1, fill.value.$2).toStringAsFixed(2)}:1',
        ];
        expect(failures, isEmpty);
      });

      // The latón fill is identified by its label (onAccent, 4,5:1 above), not
      // by its own contrast against the page, so it is left out here.
      test(
        'iconos, bordes y componentes cumplen 3:1 sobre cada superficie',
        () {
          final components = {
            'outlineStrong': c.outlineStrong,
            'accentText': c.accentText,
            'statusBlocked': c.statusBlocked,
            'statusWorking': c.statusWorking,
            'statusDone': c.statusDone,
            'statusIdle': c.statusIdle,
            'statusUnknown': c.statusUnknown,
          };
          expect(_belowMinimum(components, surfaces, 3), isEmpty);
        },
      );

      test('la pista apagada del interruptor cumple 3:1 sobre el fondo', () {
        final off = buildTheme(palette).switchTheme.trackColor!
            .resolve(const <WidgetState>{})!;
        expect(contrastRatio(off, c.bg), greaterThanOrEqualTo(3));
        expect(contrastRatio(off, c.surface), greaterThanOrEqualTo(3));
      });

      // The on track is an accent fill, which can sit under 3:1 on a light page
      // (Pergamino); its outline carries the mark instead.
      test('el borde del interruptor encendido cumple 3:1 sobre el fondo', () {
        final on = buildTheme(palette).switchTheme.trackOutlineColor!
            .resolve(const {WidgetState.selected})!;
        expect(contrastRatio(on, c.bg), greaterThanOrEqualTo(3));
        expect(contrastRatio(on, c.surface), greaterThanOrEqualTo(3));
      });
    });
  }

  test('hay seis paletas con ids únicos', () {
    expect(HerdrPalettes.all, hasLength(6));
    expect(
      HerdrPalettes.all.map((palette) => palette.id).toSet(),
      hasLength(6),
    );
  });
}
