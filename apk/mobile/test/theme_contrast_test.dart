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
  final palettes = {'oscuro': HerdrColors.dark, 'claro': HerdrColors.light};

  for (final palette in palettes.entries) {
    group('paleta ${palette.key}', () {
      final c = palette.value;
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
    });
  }
}
