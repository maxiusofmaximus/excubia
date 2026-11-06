part of 'recipes_api.dart';

/// Said where the device may only see the recipes: it lacks `APPROVE`.
const String recipePermissionText =
    'Este dispositivo solo puede ver las recetas. Concede el permiso APPROVE desde el PC para repetirlas.';

/// Said when no conversation of the chat has a project, so a recipe has nowhere to run.
const String noProjectRunText =
    'Abre una conversación con proyecto para repetir una receta.';

const String _unknownRead =
    'No se pudieron leer las recetas. Inténtalo de nuevo.';
const String _unknownRun = 'No se pudo lanzar el encargo. Inténtalo de nuevo.';

/// A run the controller refuses before it asks the host: no conversation has a project.
class RecipeNoProject implements Exception {
  const RecipeNoProject();
}

/// The wait for the grant, or null for every other failure of a read.
String? recipeReadWaitingText(Object error) =>
    isReadForbidden(error) ? readGrantWaitingText : null;

/// Spanish text for a recipe list that could not be read. A link failure keeps its own words.
String recipeReadFailureText(Object error) {
  final waiting = recipeReadWaitingText(error);
  if (waiting != null) return waiting;
  final failure = error is HostError ? error.failure : null;
  return failure == null ? _unknownRead : connectionText(failure);
}

/// True when the oficio's agent is busy right now (409 `agent_busy`).
bool recipeIsBusy(Object error) => _codeOf(error) == 'agent_busy';

/// True when the oficio has no hire left, so the way out is to hire someone.
bool recipeNeedsHire(Object error) => _codeOf(error) == 'no_hire_for_role';

/// «Ahora está ocupado; vuelve a intentarlo en N s», or the same without a number at zero.
String recipeBusyText(int seconds) => seconds > 0
    ? 'Ahora está ocupado; vuelve a intentarlo en $seconds s.'
    : 'Ahora está ocupado; vuelve a intentarlo en un momento.';

/// Spanish text for a refused run. The host's stable code is worded here; its own text never is.
String recipeRunFailureText(Object error) {
  if (error is RecipeNoProject) return noProjectRunText;
  if (error is! HostError) return _unknownRun;
  final code = jsonErrorCode(error.body);
  final failure = error.failure;
  if (failure != null && code == null) return connectionText(failure);
  return switch (code) {
    'agent_busy' => recipeBusyText(error.retryAfter ?? 0),
    'no_hire_for_role' => 'Ya no tienes a nadie contratado para este oficio. Contrata a alguien para repetirla.',
    'conversation_has_no_project' => noProjectRunText,
    'plan_not_approved' =>
      'El plan de esa conversación todavía no está aprobado.',
    _ => _statusText(error.status),
  };
}

String? _codeOf(Object error) =>
    error is HostError ? jsonErrorCode(error.body) : null;

String _statusText(int status) => switch (status) {
  403 => recipePermissionText,
  404 => 'Esa receta ya no existe. Vuelve a la lista.',
  _ => _unknownRun,
};
