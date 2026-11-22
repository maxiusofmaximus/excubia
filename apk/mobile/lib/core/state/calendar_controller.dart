import 'package:flutter/foundation.dart';
import 'package:timezone/timezone.dart' as tz;

import '../api/agenda_api.dart';
import 'agenda_list_controller.dart';
import 'calendar_time.dart';

/// Where the company's zone stands. Only [ZoneReady] shows events.
sealed class CalendarZone {
  const CalendarZone();
}

/// The zone is being read.
final class ZoneLoading extends CalendarZone {
  const ZoneLoading();
}

/// The company has no zone yet. [failure] says why the last try to fix one failed.
final class ZoneUnset extends CalendarZone {
  const ZoneUnset({this.failure, this.saving = false});

  final String? failure;
  final bool saving;
}

/// The zone could not be read; [failure] is the text to show.
final class ZoneFailed extends CalendarZone {
  const ZoneFailed(this.failure);

  final String failure;
}

/// The zone is set, but this phone's zone data does not know its name.
final class ZoneUnknown extends CalendarZone {
  const ZoneUnknown(this.name);

  final String name;
}

/// The zone works: the window on screen is [range] days from [anchor], a day of it.
final class ZoneReady extends CalendarZone {
  const ZoneReady({
    required this.name,
    required this.location,
    required this.range,
    required this.anchor,
  });

  final String name;
  final tz.Location location;
  final CalendarRange range;
  final DateTime anchor;
}

/// The calendar screen's state: the company's zone, the window on screen and the
/// events in it. The days are the company's, never the phone's. [clock] is the
/// phone's clock, which a test replaces.
class CalendarController extends ChangeNotifier {
  CalendarController(this._api, {DateTime Function()? clock})
    : _clock = clock ?? DateTime.now {
    events.addListener(notifyListeners);
  }

  final AgendaApi _api;
  final DateTime Function() _clock;

  late final AgendaListController<CalendarEvent> events =
      AgendaListController((_, after) => _page(after));

  CalendarZone _zone = const ZoneLoading();

  CalendarZone get zone => _zone;

  ZoneReady? get _ready => switch (_zone) {
    ZoneReady ready => ready,
    _ => null,
  };

  /// Reads the company's zone. A failure replaces the calendar with its reason.
  Future<void> loadZone() async {
    _zone = const ZoneLoading();
    notifyListeners();
    try {
      _showZone(await _api.companyTimezone());
    } catch (error) {
      _zone = ZoneFailed(agendaFailureText(error));
    }
    notifyListeners();
  }

  /// Fixes the company's zone, once. A failure keeps it unset and says why.
  Future<void> saveZone(String name) async {
    _zone = const ZoneUnset(saving: true);
    notifyListeners();
    try {
      _showZone(await _api.setCompanyTimezone(name));
    } catch (error) {
      _zone = ZoneUnset(failure: agendaFailureText(error));
    }
    notifyListeners();
  }

  /// Shows [range] from today on.
  void setRange(CalendarRange range) {
    final ready = _ready;
    if (ready == null) return;
    _replace(
      ready,
      range: range,
      anchor: wallDay(_clock(), ready.location),
    );
  }

  /// Moves the window by [steps] of its range: -1 back, 1 forward.
  void shift(int steps) {
    final ready = _ready;
    if (ready == null) return;
    _replace(
      ready,
      range: ready.range,
      anchor: shiftAnchor(ready.anchor, ready.range, steps),
    );
  }

  void _showZone(String? name) {
    if (name == null) {
      _zone = const ZoneUnset();
      return;
    }
    final location = zoneNamed(name);
    if (location == null) {
      _zone = ZoneUnknown(name);
      return;
    }
    _zone = ZoneReady(
      name: name,
      location: location,
      range: CalendarRange.today,
      anchor: wallDay(_clock(), location),
    );
    events.search('');
  }

  void _replace(
    ZoneReady ready, {
    required CalendarRange range,
    required DateTime anchor,
  }) {
    _zone = ZoneReady(
      name: ready.name,
      location: ready.location,
      range: range,
      anchor: anchor,
    );
    notifyListeners();
    events.search('');
  }

  /// The days on screen, as the host reads them; `null` until the zone is known.
  ({String from, String to})? get window {
    final ready = _ready;
    return ready == null ? null : windowOf(ready.anchor, ready.range);
  }

  Future<AgendaPage<CalendarEvent>> _page(String? after) {
    final days = window;
    if (days == null) {
      return Future.value(const AgendaPage<CalendarEvent>(items: [], next: null));
    }
    return _api.calendarEvents(from: days.from, to: days.to, after: after);
  }

  @override
  void dispose() {
    events.dispose();
    super.dispose();
  }
}
