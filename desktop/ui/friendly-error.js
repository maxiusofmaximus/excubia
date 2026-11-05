// The one translator from a failure to the words the operator reads. A typed refusal
// keeps its own screen where it has one; this covers what is left: JS exceptions,
// bare text from the host and refusals with no words of their own.

export const LOAD_FAILED = "No se pudo cargar. Inténtalo de nuevo.";
export const OFFLINE_TEXT = "Sin conexión con tu PC.";
export const REJECTED_TEXT = "Tu PC no aceptó la petición. Vuelve a vincularlo si sigue pasando.";

// «offline» is the lost socket; any other typed kind is the host answering with a refusal.
// Everything else is not something the operator can act on, so it gets the generic line.
export function friendlyError(error) {
  const kind = error && typeof error === "object" ? error.kind : undefined;
  if (kind === "offline") return OFFLINE_TEXT;
  if (typeof kind === "string") return REJECTED_TEXT;
  return LOAD_FAILED;
}
