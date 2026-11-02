import 'package:flutter/foundation.dart';
import 'package:herdr_notify/src/rust/api/chat/plan.dart';
import 'package:herdr_notify/src/rust/api/error.dart';
import 'package:herdr_notify/src/rust/api/onboarding/error.dart';
import 'package:herdr_notify/src/rust/api/onboarding/model.dart';
import 'package:herdr_notify/src/rust/api/onboarding/plan.dart';

import '../api/chat_transport.dart';
import '../api/host_client.dart';
import '../api/plan_runner.dart';

/// The screens of the first-run assistant, in the order the user meets them.
enum OnboardingStep { welcome, areas, level, team, confirmed, skipped }

/// The levels of `POST /v1/onboarding`. The wire name is the enum name.
enum OnboardingLevel { personal, profesional, equipo }

/// The first-run assistant («¿Para qué lo vas a usar?»). It holds what the user
/// picked; nothing is contracted until [contract] runs, and [skip] contracts
/// nobody.
class OnboardingController extends ChangeNotifier {
  OnboardingController({required this._transport});

  /// The host's limit of active hires per area (`area_full` in `errors.md`).
  static const int maxPerArea = 8;

  final ChatTransport _transport;
  RoleCatalog? _catalog;
  OnboardingStep _step = OnboardingStep.welcome;
  final Set<String> _areas = <String>{};
  OnboardingLevel _level = OnboardingLevel.profesional;
  final Set<String> _roles = <String>{};
  ErrorInfo? _failure;
  bool _busy = false;
  int _contracted = 0;
  bool _disposed = false;

  OnboardingStep get step => _step;
  bool get busy => _busy;
  ErrorInfo? get failure => _failure;
  OnboardingLevel get level => _level;

  /// How many people the last contract prepared.
  int get contracted => _contracted;

  /// The catalogue once read; `null` before the first read or after a failure.
  RoleCatalog? get catalog => _catalog;

  /// The chosen areas, in the catalogue's order.
  List<RoleArea> get chosenAreas => [
    for (final area in _catalog?.areas ?? const <RoleArea>[])
      if (_areas.contains(area.id)) area,
  ];

  bool isAreaChosen(String areaId) => _areas.contains(areaId);

  bool hasRole(String roleId) => _roles.contains(roleId);

  /// The roles of [area], in the catalogue's order.
  List<CatalogRole> rolesIn(RoleArea area) => [
    for (final role in _catalog?.roles ?? const <CatalogRole>[])
      if (role.area == area.id) role,
  ];

  /// «Contratar equipo» needs at least one person; an empty list would mean
  /// something else to the host.
  bool get canContract => !_busy && _roles.isNotEmpty;

  /// Reads the catalogue that the areas and roles come from. A failure keeps the
  /// assistant open, and calling this again retries.
  Future<void> loadCatalog() async {
    if (_busy || _catalog != null) return;
    await _run(() async {
      final catalog = parseRoleCatalog(json: await _send(planRoles()));
      if (catalog == null) throw const UnreadableAnswer();
      _catalog = catalog;
    });
  }

  /// «Continuar» on the welcome, areas and level steps.
  void advance() {
    switch (_step) {
      case OnboardingStep.welcome:
        _show(OnboardingStep.areas);
      case OnboardingStep.areas:
        if (_areas.isNotEmpty) _show(OnboardingStep.level);
      case OnboardingStep.level:
        _enterTeam();
      case OnboardingStep.team ||
          OnboardingStep.confirmed ||
          OnboardingStep.skipped:
        break;
    }
  }

  /// «Saltar» on the level step: the level is «Profesional», and the team comes next.
  void skipLevel() {
    _level = OnboardingLevel.profesional;
    _enterTeam();
  }

  void back() {
    final previous = switch (_step) {
      OnboardingStep.areas => OnboardingStep.welcome,
      OnboardingStep.level => OnboardingStep.areas,
      OnboardingStep.team => OnboardingStep.level,
      _ => null,
    };
    if (previous != null) _show(previous);
  }

  void toggleArea(String areaId) {
    if (!_areas.remove(areaId)) _areas.add(areaId);
    _failure = null;
    _notify();
  }

  void chooseLevel(OnboardingLevel level) {
    _level = level;
    _notify();
  }

  /// Adds or removes one person. Returns false, and changes nothing, when the
  /// area already has [maxPerArea] people selected; the screen says why.
  bool toggleRole(CatalogRole role) {
    if (_roles.remove(role.id)) {
      _notify();
      return true;
    }
    if (selectedIn(role.area) >= maxPerArea) return false;
    _roles.add(role.id);
    _notify();
    return true;
  }

  /// «Contratar equipo». Sends the exact selection once; a failure keeps it, so
  /// the user retries by tapping again.
  Future<void> contract() async {
    if (!canContract) return;
    final areas = [for (final area in chosenAreas) area.id];
    final roles = [
      for (final role in _catalog?.roles ?? const <CatalogRole>[])
        if (_roles.contains(role.id)) role.id,
    ];
    final count = roles.length;
    await _run(() async {
      final answer = parseOnboardingAnswer(
        json: await _send(
          planOnboardingContract(
            areas: areas,
            level: _level.name,
            roles: roles,
          ),
        ),
      );
      if (answer == null) throw const UnreadableAnswer();
      _contracted = count;
      _show(OnboardingStep.confirmed);
    });
  }

  /// «Saltar»: marks the onboarding done and contracts nobody.
  Future<void> skip() => _run(() async {
    final answer = parseOnboardingAnswer(
      json: await _send(planOnboardingSkip()),
    );
    if (answer == null) throw const UnreadableAnswer();
    _show(OnboardingStep.skipped);
  });

  /// Runs one request at a time. The error is kept for the screen, never retried.
  Future<void> _run(Future<void> Function() action) async {
    if (_busy) return;
    _busy = true;
    _failure = null;
    _notify();
    try {
      await action();
    } catch (error) {
      _failure = _explain(error);
    } finally {
      _busy = false;
      _notify();
    }
  }

  Future<String> _send(ChatPlan plan) => runPlan(_transport, plan);

  ErrorInfo _explain(Object error) {
    if (error is HostError) {
      return explainOnboardingError(status: error.status, body: error.body);
    }
    // The plan refused to build (a bad id), so the request never left the phone.
    if (error is PlanRefused) {
      return explainOnboardingError(status: 422, body: '');
    }
    if (error is UnreadableAnswer) return unreadableAnswerInfo();
    return explainOnboardingError(status: 0, body: '');
  }

  /// The team the level preselects for the chosen areas, never more than
  /// [maxPerArea] of an area.
  void _enterTeam() {
    _roles
      ..clear()
      ..addAll([
        for (final area in chosenAreas)
          ...switch (_level) {
            OnboardingLevel.personal => area.personal,
            OnboardingLevel.profesional => area.profesional,
            OnboardingLevel.equipo => area.equipo,
          }.take(maxPerArea),
      ]);
    _show(OnboardingStep.team);
  }

  /// How many people of [areaId] are selected.
  int selectedIn(String areaId) => [
    for (final role in _catalog?.roles ?? const <CatalogRole>[])
      if (role.area == areaId && _roles.contains(role.id)) role,
  ].length;

  void _show(OnboardingStep step) {
    _step = step;
    _failure = null;
    _notify();
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

/// Whether the assistant should open for this device: the onboarding is not
/// done and the host says this device may contract (`can_hire`, the `APPROVE`
/// grant). A missing field, or any failure, keeps the assistant closed, so the
/// app works as it does without it.
Future<bool> onboardingPending(ChatTransport transport) async {
  try {
    final answer = parseOnboardingAnswer(
      json: await runPlan(transport, planOnboardingRead()),
    );
    return answer != null && !answer.done && answer.canHire;
  } catch (_) {
    return false;
  }
}
