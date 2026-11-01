import 'package:flutter/material.dart';

/// The `.ics` items of the calendar's bar. [window] is the range on screen, `null`
/// until the company's zone is known, and then the visible range cannot be exported.
class CalendarIcsMenu extends StatelessWidget {
  const CalendarIcsMenu({
    super.key,
    required this.window,
    required this.onImport,
    required this.onExport,
  });

  final ({String from, String to})? window;
  final VoidCallback onImport;

  /// `null` exports every event; a window, the events of its days.
  final void Function(({String from, String to})? window) onExport;

  @override
  Widget build(BuildContext context) => PopupMenuButton<_IcsItem>(
    tooltip: 'Archivo .ics',
    icon: const Icon(Icons.more_vert),
    onSelected: (item) => switch (item) {
      _IcsItem.importFile => onImport(),
      _IcsItem.exportWindow => onExport(window),
      _IcsItem.exportAll => onExport(null),
    },
    itemBuilder: (_) => [
      const PopupMenuItem(value: _IcsItem.importFile, child: Text('Importar .ics')),
      PopupMenuItem(
        value: _IcsItem.exportWindow,
        enabled: window != null,
        child: const Text('Exportar rango visible'),
      ),
      const PopupMenuItem(value: _IcsItem.exportAll, child: Text('Exportar todo')),
    ],
  );
}

enum _IcsItem { importFile, exportWindow, exportAll }
