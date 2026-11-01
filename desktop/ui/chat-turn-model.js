// The chat's turn rules, kept free of the DOM so the tests can run them: the
// state of a message's turn, the line under the operator's last message, when
// Alfred counts as working, the «ir al final» counter and the words of a stop.

const RANK = { queued: 1, running: 2, completed: 3, failed: 3, interrupted: 3 };

export function turnRank(state) {
  return RANK[state] || 0;
}

// A turn only moves forward. A late 202, a stale timeline or a frame that
// arrives before its message never reopens a closed turn; between two closed
// states the first one seen stays.
export function advanceTurn(current, next) {
  return turnRank(next) > turnRank(current) ? next : current || null;
}

export function isOwn(entry) {
  return entry.kind === "message" && entry.message.author_kind === "user";
}

export function isAgentMessage(entry) {
  return entry.kind === "message" && entry.message.author_kind !== "user";
}

// The state a message shows: what the host sent, moved forward by the frames
// already seen for it (`turns` is keyed by message id).
export function turnOf(message, turns) {
  return advanceTurn(message.turn_state, turns[message.id]);
}

export function noteTurn(turns, id, state) {
  if (id && state) turns[id] = advanceTurn(turns[id], state);
}

// The line under the operator's last message: every failure shows, the other
// states only on the last one. No state means no line.
export function turnStatusLine(turn, alfredName, isLast) {
  if (turn === "failed") return { text: "Falló", failed: true };
  if (!turn || !isLast) return null;
  if (turn === "queued") return { text: "Enviado", failed: false };
  if (turn === "running") {
    const text = alfredName ? `${alfredName} lo está atendiendo` : "Atendiendo";
    return { text, failed: false };
  }
  if (turn === "completed") return { text: "Respondido", failed: false };
  if (turn === "interrupted") return { text: "Detenido", failed: false };
  return null;
}

// Alfred is working while the operator's last message has a turn open. An agent
// message after it closes that turn. `active_runs` decides only when that
// message carries no state (an older host).
export function turnInFlight(entries, turns, activeRuns) {
  let index = entries.length - 1;
  while (index >= 0 && !isOwn(entries[index])) index -= 1;
  if (index < 0) return activeRuns > 0;
  if (entries.slice(index + 1).some(isAgentMessage)) return false;
  const turn = turnOf(entries[index].message, turns);
  if (turn) return turn === "queued" || turn === "running";
  return activeRuns > 0;
}

// Agent messages that arrived after the reader left the bottom (`fromSeq`).
export function unseenCount(entries, fromSeq) {
  if (fromSeq === null || fromSeq === undefined) return 0;
  return entries.filter((entry) => isAgentMessage(entry) && entry.seq > fromSeq).length;
}

export function jumpLabel(count) {
  if (count === 0) return "Ir al final";
  return `Ir al final, ${count === 1 ? "1 mensaje nuevo" : `${count} mensajes nuevos`}`;
}

// The send button turns into «Detener» while a turn is open. One stop request
// is in flight at a time; a refused permission keeps the button off, with why.
export function composerMode({ inFlight, offline, sent, denied }) {
  if (!inFlight || offline) return { stop: false, disabled: false, hint: null };
  return { stop: true, disabled: Boolean(denied) || sent, hint: denied || null };
}

// Words for a failed stop. A host without the route answers 404 or 405.
export function stopFailureText(failure) {
  if (failure.kind === "forbidden") return "No tienes permiso para detener esto desde este dispositivo.";
  if (failure.kind === "notFound" || (failure.kind === "gateway" && [404, 405].includes(failure.status))) {
    return "Este equipo no admite detener todavía.";
  }
  return "No se pudo detener. Inténtalo de nuevo.";
}
