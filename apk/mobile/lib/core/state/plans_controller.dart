import 'dart:async';

import '../api/host_client.dart';
import '../api/plan_models.dart';
import '../api/plans_api.dart';
import 'active_poll_controller.dart';

/// «Planes propuestos»: the plans the team proposes, read with the team's own poll. A decision
/// answers with the host's plan, which replaces the one on screen at once.
class PlansController extends ActivePollController {
  PlansController({required this.api, super.interval = kPollInterval});

  final PlansApi api;

  List<TeamPlan> _plans = const <TeamPlan>[];
  Object? _error;
  bool _loaded = false;

  /// True once the host has refused a decision for lack of `APPROVE`; lifted when the screen
  /// shows again, since the user may have been granted it meanwhile.
  bool _denied = false;

  final Set<String> _busy = <String>{};
  final Map<String, Object> _failures = <String, Object>{};

  /// The proposals first, then the decided ones (see [sortPlans]).
  List<TeamPlan> get plans => sortPlans(_plans);

  /// How many plans wait for the user's answer.
  int get pendingCount => _plans.where((plan) => plan.isProposed).length;

  /// The last read's failure; null once a read has answered.
  Object? get error => _error;

  /// True after the first answer, even an empty one.
  bool get loaded => _loaded;

  bool get denied => _denied;

  bool isBusy(String planId) => _busy.contains(planId);

  /// The failure of the last decision on this plan, or null.
  Object? failureOf(String planId) => _failures[planId];

  @override
  void setActive(bool on) {
    if (on) _denied = false;
    super.setActive(on);
    if (on) emit();
  }

  @override
  Future<void> read() async {
    try {
      _plans = await api.plans();
      _error = null;
      _loaded = true;
    } catch (error) {
      _error = error;
    }
  }

  /// Answers one plan. Returns false when the decision did not go through; the reason is in
  /// [failureOf], or the screen is locked when the refusal was for lack of `APPROVE`.
  Future<bool> decide(
    TeamPlan plan,
    PlanDecision decision, {
    String? note,
  }) async {
    if (_denied || _busy.contains(plan.id)) return false;
    _busy.add(plan.id);
    _failures.remove(plan.id);
    emit();
    try {
      final updated = await api.answer(plan.id, decision, note: note);
      _replace(updated);
      return true;
    } catch (error) {
      _fail(plan.id, error);
      return false;
    } finally {
      _busy.remove(plan.id);
      emit();
    }
  }

  void _replace(TeamPlan updated) {
    _plans = [
      for (final plan in _plans) plan.id == updated.id ? updated : plan,
    ];
  }

  /// A refusal that is not about the plan itself is kept on the plan; the plan is read again.
  void _fail(String planId, Object error) {
    if (error is HostError && error.status == 403) {
      _denied = true;
      return;
    }
    _failures[planId] = error;
    if (error is HostError && (error.status == 404 || error.status == 409)) {
      unawaited(refresh());
    }
  }
}
