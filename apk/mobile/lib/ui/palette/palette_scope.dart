import 'package:flutter/widgets.dart';

import 'palette_controller.dart';

/// Makes the live palette reachable from any route, including the settings
/// screen, without threading it through every widget constructor.
class PaletteScope extends InheritedNotifier<PaletteController> {
  const PaletteScope({
    super.key,
    required PaletteController controller,
    required super.child,
  }) : super(notifier: controller);

  static PaletteController of(BuildContext context) {
    final scope = context.dependOnInheritedWidgetOfExactType<PaletteScope>();
    assert(scope != null, 'PaletteScope no encontrado');
    return scope!.notifier!;
  }
}
