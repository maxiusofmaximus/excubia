import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/storage/pairing_store.dart';
import 'package:excubia/ui/app.dart';
import 'package:excubia/ui/palette/palette_prefs.dart';

import 'support/memory_store.dart';

void main() {
  testWidgets('una instalación nueva abre Elige tu paleta antes de emparejar', (
    tester,
  ) async {
    final values = MemoryKeyValueStore();
    await tester.pumpWidget(
      ExcubiaApp(
        store: PairingStore(values),
        palettePrefs: PalettePrefs(values),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Elige tu paleta'), findsOneWidget);
    expect(find.text('Continuar'), findsOneWidget);
    expect(find.textContaining('Empareja este teléfono'), findsNothing);
  });
}
