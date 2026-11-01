import 'package:flutter/material.dart';

import '../../core/api/agenda_api.dart';
import '../theme.dart';

/// «N importados · M ya existían · K omitidos», and the reason of each event left out.
class CalendarIcsResult extends StatelessWidget {
  const CalendarIcsResult({super.key, required this.report});

  final IcsImportReport report;

  static String _count(int n, String one, String many) =>
      '$n ${n == 1 ? one : many}';

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Importación terminada'),
    content: SingleChildScrollView(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '${_count(report.imported, 'importado', 'importados')} · '
            '${_count(report.skippedExisting, 'ya existía', 'ya existían')} · '
            '${_count(report.skippedUnsupported, 'omitido', 'omitidos')}',
            style: context.text.body,
          ),
          for (final skip in report.details)
            Text(
              'Evento ${skip.index + 1}: ${skip.reason.label}',
              style: context.text.small,
            ),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.of(context).pop(),
        child: const Text('Cerrar'),
      ),
    ],
  );
}
