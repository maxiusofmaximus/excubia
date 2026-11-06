import '../../core/api/plan_models.dart';

// The words of «Planes propuestos», the same sentences as the desktop card.

String planStatusLabel(PlanStatus status) => switch (status) {
  PlanStatus.proposed => 'Espera tu visto bueno',
  PlanStatus.approved => 'Aprobado',
  PlanStatus.rejected => 'Rechazado',
  PlanStatus.changesRequested => 'Pediste cambios',
};

/// «1 plan espera tu visto bueno», «2 planes esperan tu visto bueno».
String planWaitingText(int count) => count == 1
    ? '1 plan espera tu visto bueno'
    : '$count planes esperan tu visto bueno';

/// What is wrong with a change note, or null when it can be sent.
String? planNoteProblem(String note) {
  final text = note.trim();
  if (text.isEmpty) return 'Escribe qué cambiarías del plan.';
  if (text.runes.length > planNoteMax) {
    return 'La nota admite $planNoteMax caracteres como máximo.';
  }
  return null;
}

const String planEmptyTitle = 'Aún no hay planes propuestos.';
const String planEmptyDetail = 'Cuando un agente proponga un plan, lo verás aquí.';
const String planLoadingLabel = 'Cargando planes';
const String planConfirmApproveText =
    'Este plan toca correo o internet. Revisa los pasos antes de aprobarlo.';
const String planRejectTitle = '¿Rechazar este plan?';
const String planRejectDetail = 'No se hará nada de lo propuesto.';

/// «Toca ficheros · Usa internet», or «Solo lee» for a step that touches nothing.
String planStepTouchText(PlanStep step) {
  if (step.touches.isEmpty) return 'Solo lee';
  final labels = {for (final touch in step.touches) planTouchLabel(touch)};
  return labels.join(' · ');
}
