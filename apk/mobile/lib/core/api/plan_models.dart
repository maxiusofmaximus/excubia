import 'json_read.dart';

// «Planes propuestos»: a plan a team member proposes and the user decides (contract `api.md`,
// F3). The host decides the status; the words here are the app's own.

/// The most a change note may say, in characters (the host refuses more).
const int planNoteMax = 300;

enum PlanStatus { proposed, approved, rejected, changesRequested }

/// A status the host does not name is a broken answer, so it is refused.
PlanStatus planStatusOf(String wire) => switch (wire) {
  'proposed' => PlanStatus.proposed,
  'approved' => PlanStatus.approved,
  'rejected' => PlanStatus.rejected,
  'changes_requested' => PlanStatus.changesRequested,
  _ => throw FormatException('estado de plan desconocido: $wire'),
};

/// What a step touches. A kind the app does not know reads as [other], which still counts
/// in the summary but never as a mail or internet step.
enum PlanTouch { files, network, mail, tools, other }

PlanTouch planTouchOf(String wire) => switch (wire) {
  'files' => PlanTouch.files,
  'network' => PlanTouch.network,
  'mail' => PlanTouch.mail,
  'tools' => PlanTouch.tools,
  _ => PlanTouch.other,
};

/// The plain label of a touch, the one shown on a step.
String planTouchLabel(PlanTouch touch) => switch (touch) {
  PlanTouch.files => 'Toca ficheros',
  PlanTouch.network => 'Usa internet',
  PlanTouch.mail => 'Toca correo',
  PlanTouch.tools => 'Usa herramientas',
  PlanTouch.other => 'Hace otra cosa',
};

/// The word a touch takes inside the summary sentence.
String _touchWord(PlanTouch touch) => switch (touch) {
  PlanTouch.files => 'ficheros',
  PlanTouch.network => 'internet',
  PlanTouch.mail => 'correo',
  PlanTouch.tools => 'herramientas',
  PlanTouch.other => 'otra cosa',
};

/// The order of the words in a summary, so the same plan always reads the same way.
const List<PlanTouch> _touchOrder = [
  PlanTouch.files,
  PlanTouch.mail,
  PlanTouch.network,
  PlanTouch.tools,
  PlanTouch.other,
];

enum PlanDecision { approve, reject, requestChanges }

class PlanStep {
  const PlanStep({required this.title, required this.touches});

  factory PlanStep.fromJson(Map<String, Object?> json) => PlanStep(
    title: jsonText(json, 'title'),
    touches: [for (final wire in jsonStrings(json, 'touches')) planTouchOf(wire)],
  );

  final String title;
  final List<PlanTouch> touches;
}

class TeamPlan {
  const TeamPlan({
    required this.id,
    required this.status,
    required this.steps,
    required this.summary,
    this.note,
    this.delegationId,
    this.decidedBy,
    this.createdAt,
    this.updatedAt,
    this.decidedAt,
  });

  factory TeamPlan.fromJson(Map<String, Object?> json) => TeamPlan(
    id: jsonText(json, 'id'),
    status: planStatusOf(jsonText(json, 'status')),
    steps: [
      for (final item in jsonList(json, 'steps'))
        PlanStep.fromJson(jsonObject(item)),
    ],
    summary: jsonOptionalText(json, 'summary') ?? '',
    note: jsonOptionalText(json, 'note'),
    delegationId: jsonOptionalText(json, 'delegation_id'),
    decidedBy: jsonOptionalText(json, 'decided_by'),
    createdAt: _timeOf(json, 'created_at'),
    updatedAt: _timeOf(json, 'updated_at'),
    decidedAt: _timeOf(json, 'decided_at'),
  );

  final String id;
  final PlanStatus status;
  final List<PlanStep> steps;

  /// The host's own sentence, shown as it came. It says who proposes the plan.
  final String summary;
  final String? note;
  final String? delegationId;
  final String? decidedBy;
  final DateTime? createdAt;
  final DateTime? updatedAt;
  final DateTime? decidedAt;

  bool get isProposed => status == PlanStatus.proposed;
}

DateTime? _timeOf(Map<String, Object?> json, String key) =>
    DateTime.tryParse(jsonOptionalText(json, key) ?? '');

/// The plans in the host's order. The list it is given is left as it came.
List<TeamPlan> teamPlansFrom(Map<String, Object?> envelope) => [
  for (final item in jsonList(envelope, 'plans'))
    TeamPlan.fromJson(jsonObject(item)),
];

/// The plan one decision answered with.
TeamPlan teamPlanFrom(Map<String, Object?> envelope) =>
    TeamPlan.fromJson(jsonObject(envelope['plan']));

/// The proposals first, in the host's order; then the decided ones, newest decision first.
List<TeamPlan> sortPlans(Iterable<TeamPlan> plans) {
  final indexed = plans.indexed.toList()
    ..sort((a, b) {
      final ranked = _rank(a.$2).compareTo(_rank(b.$2));
      if (ranked != 0) return ranked;
      if (a.$2.isProposed) return a.$1.compareTo(b.$1);
      final decidedA = a.$2.updatedAt ?? DateTime.fromMillisecondsSinceEpoch(0);
      final decidedB = b.$2.updatedAt ?? DateTime.fromMillisecondsSinceEpoch(0);
      return decidedB.compareTo(decidedA);
    });
  return [for (final (_, plan) in indexed) plan];
}

int _rank(TeamPlan plan) => plan.isProposed ? 0 : 1;

/// «4 pasos · toca ficheros e internet», «1 paso · solo lee». Each touch is named once.
String planStepsLine(List<PlanStep> steps) {
  final count = steps.length == 1 ? '1 paso' : '${steps.length} pasos';
  final named = {for (final step in steps) ...step.touches};
  final kinds = [
    for (final touch in _touchOrder)
      if (named.contains(touch)) _touchWord(touch),
  ];
  final touch = kinds.isEmpty ? 'solo lee' : 'toca ${_joinWords(kinds)}';
  return '$count · $touch';
}

// «ficheros», «ficheros e internet», «correo, ficheros y herramientas»: the Spanish «e»
// goes before an «i».
String _joinWords(List<String> words) {
  if (words.length == 1) return words.single;
  final last = words.last;
  final connector = RegExp(r'^i', caseSensitive: false).hasMatch(last) ? 'e' : 'y';
  return '${words.sublist(0, words.length - 1).join(', ')} $connector $last';
}

/// Only a mail or internet step asks the user to read the steps once more.
bool needsExtraConfirm(TeamPlan plan) => plan.steps.any(
  (step) =>
      step.touches.contains(PlanTouch.mail) ||
      step.touches.contains(PlanTouch.network),
);
