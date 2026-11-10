import 'package:flutter/foundation.dart';
import 'package:excubia/src/rust/api/chat/plan.dart';
import 'package:excubia/src/rust/api/error.dart';
import 'package:excubia/src/rust/api/onboarding/error.dart';
import 'package:excubia/src/rust/api/onboarding/model.dart';
import 'package:excubia/src/rust/api/onboarding/plan.dart';

import '../api/chat_transport.dart';
import '../api/host_client.dart';
import '../api/plan_runner.dart';

/// The professionals the user has contracted (`GET /v1/hires`) and the catalogue
/// they come from (`GET /v1/roles`). Each change is one tap of the user and the
/// list is read again afterwards; a failure is kept for the screen and never
/// retried by itself.
class HiresController extends ChangeNotifier {
  HiresController({required this._transport});

  final ChatTransport _transport;
  RoleCatalog? _catalog;
  List<Hire> _hires = const <Hire>[];
  bool _canHire = false;
  bool _busy = false;
  ErrorInfo? _failure;
  bool _disposed = false;

  /// The host's `can_hire`: this device holds `APPROVE`. Without it the team is
  /// only shown, never changed.
  bool get canHire => _canHire;
  bool get busy => _busy;
  ErrorInfo? get failure => _failure;

  /// The contracted people, in the host's order. Dismissed ones are not here.
  List<Hire> get hires => _hires;

  /// The catalogue once read; `null` before the first read or after a failure.
  RoleCatalog? get catalog => _catalog;

  bool isHired(String roleId) => _hires.any((hire) => hire.role == roleId);

  CatalogRole? roleById(String roleId) {
    for (final role in _catalog?.roles ?? const <CatalogRole>[]) {
      if (role.id == roleId) return role;
    }
    return null;
  }

  RoleArea? areaById(String areaId) {
    for (final area in _catalog?.areas ?? const <RoleArea>[]) {
      if (area.id == areaId) return area;
    }
    return null;
  }

  /// The roles of [area], in the catalogue's order.
  List<CatalogRole> rolesIn(RoleArea area) => [
    for (final role in _catalog?.roles ?? const <CatalogRole>[])
      if (role.area == area.id) role,
  ];

  /// The contracted people grouped by area, in the catalogue's order. An area the
  /// catalogue does not know (or no catalogue yet) keeps the host's order, under
  /// its own id.
  List<({String label, List<Hire> hires})> hiresByArea() {
    final byArea = <String, List<Hire>>{};
    for (final hire in _hires) {
      byArea.putIfAbsent(hire.area, () => <Hire>[]).add(hire);
    }
    final known = _catalog?.areas ?? const <RoleArea>[];
    final ordered = [
      for (final area in known)
        if (byArea.containsKey(area.id)) area.id,
      ...byArea.keys.where((id) => known.every((area) => area.id != id)),
    ];
    return [
      for (final id in ordered)
        (label: areaById(id)?.label ?? 'Equipo', hires: byArea[id]!),
    ];
  }

  /// The user closed the error message; nothing else changes.
  void clearFailure() {
    if (_failure == null) return;
    _failure = null;
    _notify();
  }

  /// Reads the catalogue (once) and the hires. Calling it again retries.
  Future<bool> load() => _run(() async {
    if (_catalog == null) {
      final catalog = parseRoleCatalog(json: await _send(planRoles()));
      if (catalog == null) throw const UnreadableAnswer();
      _catalog = catalog;
    }
    await _fetchHires();
  });

  /// Re-reads the list alone, e.g. after the user pulls to refresh.
  Future<bool> refresh() => _run(_fetchHires);

  /// Contracts [role] with its own name and avatar. Returns the new hire, or
  /// `null` when nothing was contracted (already hired, busy, or a failure).
  Future<Hire?> hire(CatalogRole role) async {
    if (isHired(role.id)) return null;
    Hire? created;
    final ok = await _run(() async {
      created = _hireFrom(await _send(planHire(role: role.id)));
      await _fetchHires();
    });
    return ok ? created : null;
  }

  /// Renames [hire]. The Rust plan checks the name first and the host has the
  /// final word; the list is read again either way.
  Future<bool> rename(Hire hire, String name) => _run(() async {
    _hireFrom(await _send(planHireRename(hireId: hire.id, name: name)));
    await _fetchHires();
  });

  /// Gives [hire] the character [avatar], or none (`null`, the initial). The
  /// host reads a `null` avatar as "no change", so the answer is compared with
  /// what was asked and a mismatch is said to the user.
  Future<bool> changeAvatar(Hire hire, String? avatar) => _run(() async {
    final changed = _hireFrom(
      await _send(planHireAvatar(hireId: hire.id, avatar: avatar)),
    );
    await _fetchHires();
    if (changed.avatar != avatar) {
      _failure = ErrorInfo(
        message: 'El equipo no ha guardado el personaje.',
        action: 'Vuelve a intentarlo más tarde.',
        kind: FailureKind.retry,
      );
    }
  });

  /// Dismisses [hire]. Dismissing it again changes nothing.
  Future<bool> dismiss(Hire hire) => _run(() async {
    _hireFrom(await _send(planHireDismiss(hireId: hire.id)));
    await _fetchHires();
  });

  Future<void> _fetchHires() async {
    final list = parseHires(
      json: await _send(planHires(includeRemoved: false)),
    );
    if (list == null) throw const UnreadableAnswer();
    _hires = list.hires;
    _canHire = list.canHire;
    _notify();
  }

  Hire _hireFrom(String json) {
    final hire = parseHire(json: json);
    if (hire == null) throw const UnreadableAnswer();
    return hire;
  }

  /// Runs one change at a time. A failure is kept for the screen, not retried.
  /// The action may also set [failure] itself when the host answered, but said
  /// something other than what was asked.
  Future<bool> _run(Future<void> Function() action) async {
    if (_busy) return false;
    _busy = true;
    _failure = null;
    _notify();
    try {
      await action();
      return _failure == null;
    } catch (error) {
      _failure = _explain(error);
      return false;
    } finally {
      _busy = false;
      _notify();
    }
  }

  Future<String> _send(ChatPlan plan) => runPlan(_transport, plan);

  ErrorInfo _explain(Object error) {
    if (error is HostError) {
      return explainHireError(status: error.status, body: error.body);
    }
    if (error is PlanRefused) {
      return error.problem ?? explainHireError(status: 422, body: '');
    }
    if (error is UnreadableAnswer) return unreadableAnswerInfo();
    return explainHireError(status: 0, body: '');
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
