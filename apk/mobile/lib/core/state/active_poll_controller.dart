import 'dart:async';

import 'package:flutter/foundation.dart';

/// How often a shown list is read again.
const Duration kPollInterval = Duration(seconds: 5);

/// A list read every [interval] while its screen is showing ([setActive]). One read at a
/// time: a read asked while one is in flight is dropped, not queued. A stopped screen
/// starts none. Subclasses keep their own failures; [read] never throws.
abstract class ActivePollController extends ChangeNotifier {
  ActivePollController({required this.interval});

  final Duration interval;

  bool _active = false;
  bool _busy = false;
  bool _disposed = false;
  Timer? _timer;

  /// One read of the list. A failure is kept by the subclass, not thrown.
  @protected
  Future<void> read();

  /// Polls while [on]: a read now, then one [interval] after each answer. Off stops the timer.
  void setActive(bool on) {
    if (on == _active) return;
    _active = on;
    if (on) {
      unawaited(refresh());
    } else {
      _timer?.cancel();
      _timer = null;
    }
  }

  /// Reads the list once, unless a read is already in flight.
  Future<void> refresh() async {
    if (_busy) return;
    _busy = true;
    try {
      await read();
    } finally {
      _busy = false;
      emit();
      if (_active && !_disposed) _schedule();
    }
  }

  /// Tells the listeners, unless the controller is gone.
  @protected
  void emit() {
    if (!_disposed) notifyListeners();
  }

  void _schedule() {
    _timer?.cancel();
    _timer = Timer(interval, () => unawaited(refresh()));
  }

  @override
  void dispose() {
    _disposed = true;
    _timer?.cancel();
    _timer = null;
    super.dispose();
  }
}
