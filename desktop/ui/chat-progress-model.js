// The step of the open turn («chat.progress»), kept free of the DOM so the tests
// can run it. The text is written by the model: data, one plain line, at most 120
// characters. Progress is stored per conversation id, so an event that arrives
// before its conversation waits for it.

export const PROGRESS_MAX = 120;
export const ANNOUNCE_GAP_MS = 5000;

// Control characters and runs of whitespace become one space. Nothing is parsed:
// markup and links stay as the text the model wrote. Empty means no line.
export function cleanProgress(text) {
  if (typeof text !== "string") return null;
  const line = text.replace(/[\u0000-\u001F\u007F-\u009F]/g, " ").replace(/\s+/g, " ").trim();
  if (!line) return null;
  const chars = [...line];
  if (chars.length <= PROGRESS_MAX) return line;
  return `${chars.slice(0, PROGRESS_MAX - 1).join("").trimEnd()}…`;
}

// A frame for one run. A frame older than the stored one (a lower revision) never
// replaces it. A null line closes only its own run: a close of another run is late.
export function applyProgress(store, event) {
  const id = event.conversation_id;
  if (!id) return;
  const current = store[id];
  if (current && event.revision <= current.revision) return;
  const text = cleanProgress(event.text);
  if (!text) {
    if (current && (current.runId === null || current.runId === event.run_id)) delete store[id];
    return;
  }
  store[id] = {
    runId: event.run_id || null,
    revision: event.revision,
    text,
    said: current ? current.said : null,
    at: current ? current.at : 0,
  };
}

// A conversation row from the host. With a line it seeds the store, unless a frame
// already set one (frames are newer). Without a line the run is over: the row clears it.
export function syncProgress(store, conversation) {
  const text = cleanProgress(conversation.progress);
  if (!text) {
    delete store[conversation.id];
    return;
  }
  if (!store[conversation.id]) {
    store[conversation.id] = { runId: null, revision: 0, text, said: null, at: 0 };
  }
}

export function progressText(store, conversationId) {
  const entry = store[conversationId];
  return entry ? entry.text : null;
}

// The step to show for the open conversation: only while its turn is in flight.
export function openStep(chat) {
  return chat.waiting && chat.conversation ? progressText(chat.progress, chat.conversation.id) : null;
}

// The live region reads a line out once per change, and never sooner than
// ANNOUNCE_GAP_MS after the last reading. A change inside that gap is read later,
// on the first render after the gap, if the line is still the same.
export function announcement(entry, now) {
  if (!entry || entry.text === entry.said) return "off";
  if (entry.at && now - entry.at < ANNOUNCE_GAP_MS) return "off";
  entry.said = entry.text;
  entry.at = now;
  return "polite";
}
