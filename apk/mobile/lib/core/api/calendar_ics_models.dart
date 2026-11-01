part of 'agenda_api.dart';

/// The largest `.ics` the host takes, in bytes (512 KiB).
const int icsMaxBytes = 512 * 1024;

/// What the host did with an imported `.ics`: counts, and the reason of each event it
/// left out, at most 50 of them (the rest are only counted).
class IcsImportReport {
  const IcsImportReport({
    required this.imported,
    required this.skippedExisting,
    required this.skippedUnsupported,
    required this.details,
  });

  factory IcsImportReport.fromJson(Map<String, Object?> json) =>
      IcsImportReport(
        imported: jsonInt(json, 'imported'),
        skippedExisting: jsonInt(json, 'skipped_existing'),
        skippedUnsupported: jsonInt(json, 'skipped_unsupported'),
        details: [
          for (final item in jsonList(json, 'details'))
            IcsSkip.fromJson(jsonObject(item)),
        ],
      );

  final int imported;
  final int skippedExisting;
  final int skippedUnsupported;
  final List<IcsSkip> details;
}

/// An event the import left out: its place in the file, from 0, and why.
class IcsSkip {
  const IcsSkip({required this.index, required this.reason});

  factory IcsSkip.fromJson(Map<String, Object?> json) => IcsSkip(
    index: jsonInt(json, 'index'),
    reason: IcsReason.fromWire(jsonText(json, 'reason')),
  );

  final int index;
  final IcsReason reason;
}

/// Why an event was left out, in the words the person reads.
enum IcsReason {
  exists('exists', 'Ya existía'),
  unsupportedRecurrence('unsupported_recurrence', 'Repetición no soportada'),
  exceptionInstance('exception_instance', 'Ocurrencia suelta de una serie'),
  noStart('no_start', 'Sin fecha de inicio'),
  invalidTime('invalid_time', 'Hora no válida'),
  invalidText('invalid_text', 'Texto no válido'),
  invalidRecurrence('invalid_recurrence', 'Repetición no válida'),
  calendarLimit('calendar_limit', 'Límite de eventos alcanzado'),
  other('', 'Motivo no reconocido');

  const IcsReason(this.wire, this.label);

  final String wire;
  final String label;

  /// A reason this app does not know is [other]: the import went on without it.
  static IcsReason fromWire(String wire) => values.firstWhere(
    (reason) => reason.wire == wire,
    orElse: () => other,
  );
}
