// The dictation button in the chat composer. A tap starts a note; a second tap (or
// Ctrl+Shift+M) stops it and sends it to the host. The text lands at the cursor and is
// never sent on its own. The audio stays in memory: nothing is written or logged.
import { $, invoke } from "./core.js";
import { element } from "./chatdom.js";
import { failure } from "./chatstate.js";
import { startCapture } from "./voice-recorder.js";
import {
  EMPTY_TEXT,
  atLimit,
  concatBlocks,
  elapsedText,
  floatToPcm16,
  isDictationShortcut,
  micFailureText,
  peakLevel,
  resample,
  spliceText,
  voiceFailureText,
  wavFile,
} from "./voice-model.js";

const CONSENT_TEXT = "Tu voz se graba aquí y se envía al PC de tu cuenta para transcribirla. No se guarda.";
const KEYS_TEXT = "Esc cancela · el texto va al cuadro, no se envía";
const MIC_ICON =
  '<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" ' +
  'stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
  '<rect x="9" y="3" width="6" height="11" rx="3"></rect><path d="M5 11a7 7 0 0 0 14 0"></path>' +
  '<path d="M12 18v3"></path></svg>';

// phase: idle | starting | recording | transcribing | error. `current` is the note being
// captured; it is dropped the moment the phase leaves recording, so a late block is ignored.
let phase = "idle";
let current = null;
let explained = false;
let blocked = false;
let button = null;
let bar = null;
let status = null;
let note = null;
let meter = null;
let cancel = null;

export function installVoice() {
  const css = element("link");
  css.rel = "stylesheet";
  css.href = "chatvoice.css";
  document.head.append(css);

  button = element("button", "attach-btn voice-btn");
  button.type = "button";
  button.id = "btn-voice";
  button.innerHTML = MIC_ICON;
  $("btn-send").before(button);
  button.addEventListener("click", toggle);

  bar = element("div", "voice-bar");
  bar.id = "voice-bar";
  meter = element("span", "voice-meter");
  meter.setAttribute("aria-hidden", "true");
  meter.append(element("i"));
  status = element("span", "voice-status");
  status.setAttribute("aria-live", "polite");
  note = element("span", "voice-note");
  cancel = element("button", "btn secondary btn-small");
  cancel.type = "button";
  cancel.addEventListener("click", discard);
  bar.append(meter, status, note, cancel);
  $("chat-composer").prepend(bar);

  document.addEventListener("keydown", (event) => {
    if (isDictationShortcut(event)) {
      if ($("view-chat").classList.contains("hidden")) return;
      event.preventDefault();
      toggle();
    } else if (event.key === "Escape") {
      discard();
    }
  });
  show("idle");
}

// The composer is blocked when the chat is archived or the host is offline.
export function setVoiceBlocked(value) {
  if (!button) return;
  blocked = value;
  if (value) discard();
  button.disabled = value || phase === "starting" || phase === "transcribing";
}

function toggle() {
  if (phase === "recording") finish();
  else if (phase === "idle" || phase === "error") start();
}

async function start() {
  if (blocked) return;
  show("starting", "Pidiendo el micrófono…");
  note.textContent = explained ? KEYS_TEXT : CONSENT_TEXT;
  const take = { blocks: [], count: 0, rate: 0, capture: null };
  current = take;
  try {
    take.capture = await startCapture((block) => onBlock(take, block));
  } catch (error) {
    if (current === take) current = null;
    show("error", micFailureText(error?.name));
    return;
  }
  if (current !== take) {
    // Cancelled while the microphone was opening.
    await take.capture.stop();
    return;
  }
  take.rate = take.capture.rate;
  explained = true;
  show("recording", "Grabando · 0:00");
}

function onBlock(take, block) {
  if (current !== take || phase !== "recording") return;
  take.blocks.push(block);
  take.count += block.length;
  if (atLimit(take.count, take.rate)) {
    finish();
    return;
  }
  meter.style.setProperty("--level", String(peakLevel(block)));
  // Only a change of second is read out; the same words would be announced again.
  const label = `Grabando · ${elapsedText(take.count, take.rate)}`;
  if (status.textContent !== label) status.textContent = label;
}

async function finish() {
  const take = current;
  current = null;
  show("transcribing", "Transcribiendo…");
  await take.capture.stop();
  const pcm = floatToPcm16(resample(concatBlocks(take.blocks), take.rate));
  if (pcm.length === 0) {
    show("error", EMPTY_TEXT);
    return;
  }
  try {
    const { text } = await invoke("voice_transcribe", wavFile(pcm));
    if (!text.trim()) {
      show("error", EMPTY_TEXT);
      return;
    }
    insertText(text);
    show("idle");
  } catch (error) {
    show("error", voiceFailureText(failure(error)));
  }
}

// Stops a note in progress without sending it, or closes an error message.
async function discard() {
  if (phase === "error") {
    show("idle");
    return;
  }
  if (phase !== "recording" && phase !== "starting") return;
  const take = current;
  current = null;
  show("idle");
  if (take && take.capture) await take.capture.stop();
}

function insertText(text) {
  const input = $("chat-input");
  const start = input.selectionStart ?? input.value.length;
  const end = input.selectionEnd ?? start;
  const next = spliceText(input.value, start, end, text);
  input.value = next.value;
  input.setSelectionRange(next.caret, next.caret);
  // The composer's own listeners (height, draft) follow a typed change.
  input.dispatchEvent(new Event("input"));
  input.focus();
}

function show(next, text = "") {
  phase = next;
  bar.hidden = next === "idle";
  bar.dataset.state = next;
  status.textContent = text;
  const recording = next === "recording";
  button.classList.toggle("is-recording", recording);
  button.setAttribute("aria-pressed", String(recording));
  button.setAttribute("aria-label", recording ? "Parar y transcribir" : "Dictar un mensaje");
  button.title = recording ? "Parar y transcribir (Ctrl+Mayús+M)" : "Dictar (Ctrl+Mayús+M)";
  button.disabled = blocked || next === "starting" || next === "transcribing";
  note.hidden = next !== "starting" && next !== "recording";
  cancel.hidden = next === "transcribing" || next === "starting";
  cancel.textContent = next === "error" ? "Cerrar" : "Cancelar";
  if (next === "idle") meter.style.setProperty("--level", "0");
}
