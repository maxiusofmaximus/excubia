// Dictation, pure: the 16 kHz resampling, the PCM16 mono WAV the host takes, the 60 s
// limit, the text at the cursor and the words of each refusal. No DOM, no audio API.

export const TARGET_RATE = 16000;
export const MAX_SECONDS = 60;
export const EMPTY_TEXT = "No se oyó nada";

const WAV_HEADER_BYTES = 44;

// shortcut: linear interpolation with no anti-alias filter; speech sits under 8 kHz, upgrade to a windowed-sinc filter if the recognizer misses consonants.
export function resample(samples, fromRate, toRate = TARGET_RATE) {
  const length = Math.floor((samples.length * toRate) / fromRate);
  const out = new Float32Array(length);
  const step = fromRate / toRate;
  for (let index = 0; index < length; index += 1) {
    const position = index * step;
    const low = Math.floor(position);
    const high = Math.min(low + 1, samples.length - 1);
    const fraction = position - low;
    out[index] = samples[low] * (1 - fraction) + samples[high] * fraction;
  }
  return out;
}

export function floatToPcm16(samples) {
  const out = new Int16Array(samples.length);
  for (let index = 0; index < samples.length; index += 1) {
    const value = Math.max(-1, Math.min(1, samples[index]));
    out[index] = Math.round(value < 0 ? value * 0x8000 : value * 0x7fff);
  }
  return out;
}

// The 44-byte RIFF header of a PCM16 mono file, then the samples.
export function wavFile(pcm, rate = TARGET_RATE) {
  const dataBytes = pcm.byteLength;
  const buffer = new ArrayBuffer(WAV_HEADER_BYTES + dataBytes);
  const view = new DataView(buffer);
  const text = (offset, word) => {
    for (let index = 0; index < word.length; index += 1) view.setUint8(offset + index, word.charCodeAt(index));
  };
  text(0, "RIFF");
  view.setUint32(4, 36 + dataBytes, true);
  text(8, "WAVE");
  text(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, rate, true);
  view.setUint32(28, rate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  text(36, "data");
  view.setUint32(40, dataBytes, true);
  new Uint8Array(buffer, WAV_HEADER_BYTES).set(new Uint8Array(pcm.buffer, pcm.byteOffset, dataBytes));
  return buffer;
}

export function atLimit(sampleCount, rate) {
  return sampleCount >= rate * MAX_SECONDS;
}

export function elapsedText(sampleCount, rate) {
  const seconds = Math.floor(sampleCount / rate);
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

export function peakLevel(samples) {
  let peak = 0;
  for (let index = 0; index < samples.length; index += 1) peak = Math.max(peak, Math.abs(samples[index]));
  return Math.min(1, peak);
}

export function concatBlocks(blocks) {
  const total = blocks.reduce((sum, block) => sum + block.length, 0);
  const out = new Float32Array(total);
  let offset = 0;
  for (const block of blocks) {
    out.set(block, offset);
    offset += block.length;
  }
  return out;
}

// The text goes where the cursor was, with one space where a word meets it. The caret
// lands after the inserted words.
export function spliceText(value, start, end, insert) {
  const text = insert.trim();
  if (!text) return { value, caret: end };
  const before = value.slice(0, start);
  const after = value.slice(end);
  const lead = before && !/\s$/.test(before) ? " " : "";
  const tail = after && !/^\s/.test(after) ? " " : "";
  const head = `${before}${lead}${text}`;
  return { value: `${head}${tail}${after}`, caret: head.length };
}

const VOICE_TEXT = new Map([
  ["busy", "Ya se está transcribiendo otra nota, prueba en 2 s"],
  ["forbidden", "Este dispositivo no tiene permiso para dictar"],
  ["tooLarge", "La nota es demasiado larga. El máximo es de 60 s."],
  ["unavailable", "El dictado no está disponible en el PC"],
  ["offline", "Sin conexión con el PC. Revisa la red y vuelve a intentarlo."],
  ["protocol", "No se pudo preparar la conexión con el PC."],
  ["rejected", "El PC no pudo leer la nota. Vuelve a intentarlo."],
]);

export function voiceFailureText(failure) {
  return VOICE_TEXT.get(failure.kind) ?? "No se pudo transcribir la nota. Vuelve a intentarlo.";
}

export function micFailureText(name) {
  if (name === "NotAllowedError" || name === "SecurityError") {
    return "Windows no deja usar el micrófono. Abre Configuración › Privacidad y seguridad › Micrófono, permite el acceso a la app y vuelve a intentarlo.";
  }
  if (name === "NotFoundError") return "No hay ningún micrófono conectado al equipo.";
  return "No se pudo abrir el micrófono.";
}

export function isDictationShortcut(event) {
  return event.ctrlKey && event.shiftKey && !event.altKey && !event.metaKey && event.code === "KeyM";
}
