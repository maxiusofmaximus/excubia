import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/ui/palette/palette.dart';

import 'support/contrast.dart';

void main() {
  group('tokens comic de cada paleta', () {
    for (final palette in HerdrPalettes.all) {
      test('${palette.name}: pares de texto ≥ 4.5:1', () {
        final c = palette.colors;
        final pairs = {
          'hot/onHot': (c.hot, c.onHot),
          'pale/onPale': (c.pale, c.onPale),
          'deep/onDeep': (c.deep, c.onDeep),
          'peligro onAttention/statusBlocked': (c.statusBlocked, c.onAttention),
        };
        final failures = [
          for (final pair in pairs.entries)
            if (contrastRatio(pair.value.$1, pair.value.$2) < 4.5)
              '${pair.key}: ${contrastRatio(pair.value.$1, pair.value.$2).toStringAsFixed(2)}:1',
        ];
        expect(failures, isEmpty);
      });

      test('${palette.name}: sombra dura y punteado son translúcidos', () {
        final c = palette.colors;
        expect(c.hard.a, lessThan(1));
        expect(c.dot.a, lessThan(1));
      });
    }
  });

  test('Telón existe, es nueva y tiene su nombre', () {
    final telon = HerdrPalettes.all.where((p) => p.id == HerdrPalettes.telonId);
    expect(telon, hasLength(1));
    expect(telon.single.name, 'Telón');
    expect(telon.single.isNew, isTrue);
  });

  test('solo Telón va marcada como nueva', () {
    final nuevas = HerdrPalettes.all.where((p) => p.isNew).map((p) => p.id);
    expect(nuevas.toList(), [HerdrPalettes.telonId]);
  });
}
