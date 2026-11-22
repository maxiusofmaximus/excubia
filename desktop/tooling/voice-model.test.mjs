// Run with: node --test tooling/voice-model.test.mjs
// Dictation: the resampling to 16 kHz, the PCM16 mono WAV, the 60 s limit, the
// insertion at the cursor, the words of each refusal and the shortcut.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  EMPTY_TEXT,
  MAX_SECONDS,
  TARGET_RATE,
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
} from "../ui/voice-model.js";

test("the host takes 16 kHz and the limit is 60 s", () => {
  assert.equal(TARGET_RATE, 16000);
  assert.equal(MAX_SECONDS, 60);
});

test("resampling 48 kHz and 44.1 kHz input gives 16 kHz samples per second", () => {
  assert.equal(resample(new Float32Array(48000), 48000).length, 16000);
  assert.equal(resample(new Float32Array(44100), 44100).length, 16000);
  assert.equal(resample(new Float32Array(44100 * 2), 44100).length, 32000);
});

test("a constant signal stays constant after resampling", () => {
  const out = resample(new Float32Array(4410).fill(0.5), 44100);
  assert.ok(out.every((value) => Math.abs(value - 0.5) < 1e-6));
});

test("a 1 kHz tone keeps its shape through 48 kHz to 16 kHz", () => {
  const input = new Float32Array(48000);
  for (let index = 0; index < input.length; index += 1) {
    input[index] = Math.sin((2 * Math.PI * 1000 * index) / 48000);
  }
  const out = resample(input, 48000);
  let worst = 0;
  for (let index = 0; index < out.length; index += 1) {
    const expected = Math.sin((2 * Math.PI * 1000 * index) / 16000);
    worst = Math.max(worst, Math.abs(out[index] - expected));
  }
  assert.ok(worst < 0.01, `error ${worst}`);
});

test("the same rate copies the samples", () => {
  assert.deepEqual([...resample(Float32Array.of(0.1, -0.2, 0.3), 16000)], [
    Math.fround(0.1),
    Math.fround(-0.2),
    Math.fround(0.3),
  ]);
});

test("floats become PCM16 with the clamp at both ends", () => {
  assert.deepEqual(
    [...floatToPcm16(Float32Array.of(0, 0.5, -0.5, 1, -1, 2, -2))],
    [0, 16384, -16384, 32767, -32768, 32767, -32768],
  );
});

test("the WAV header is the 44 bytes of a PCM16 mono 16 kHz file", () => {
  const wav = wavFile(Int16Array.of(1, -2, 3));
  assert.equal(wav.byteLength, 44 + 6);
  const header = Buffer.from(wav, 0, 44).toString("hex");
  assert.equal(
    header,
    [
      "52494646", // RIFF
      "2a000000", // size 36 + 6
      "57415645", // WAVE
      "666d7420", // fmt
      "10000000", // 16
      "0100", // PCM
      "0100", // mono
      "803e0000", // 16000
      "007d0000", // 32000 bytes per second
      "0200", // block align
      "1000", // 16 bits
      "64617461", // data
      "06000000", // 6 bytes
    ].join(""),
  );
  assert.deepEqual([...new Int16Array(wav, 44)], [1, -2, 3]);
});

test("the limit is reached at 60 s of the device rate", () => {
  assert.equal(atLimit(48000 * 60 - 1, 48000), false);
  assert.equal(atLimit(48000 * 60, 48000), true);
  assert.equal(atLimit(44100 * 60, 44100), true);
});

test("the elapsed time reads as m:ss", () => {
  assert.equal(elapsedText(0, 16000), "0:00");
  assert.equal(elapsedText(16000 * 59.9, 16000), "0:59");
  assert.equal(elapsedText(16000 * 75, 16000), "1:15");
});

test("the level is the loudest sample, from 0 to 1", () => {
  assert.equal(peakLevel(Float32Array.of(0.1, -0.5, 0.2)), 0.5);
  assert.equal(peakLevel(new Float32Array(0)), 0);
  assert.equal(peakLevel(Float32Array.of(3)), 1);
});

test("blocks join in order", () => {
  assert.deepEqual([...concatBlocks([Float32Array.of(1, 2), Float32Array.of(3)])], [1, 2, 3]);
});

test("the text goes at the cursor with a space where a word meets it", () => {
  assert.deepEqual(spliceText("hola mundo", 5, 5, "buen"), { value: "hola buen mundo", caret: 9 });
  assert.deepEqual(spliceText("hola mundo", 10, 10, "ok"), { value: "hola mundo ok", caret: 13 });
  assert.deepEqual(spliceText("", 0, 0, " hola "), { value: "hola", caret: 4 });
});

test("a selection is replaced", () => {
  assert.deepEqual(spliceText("hola mundo", 0, 4, "adiós"), { value: "adiós mundo", caret: 5 });
});

test("each refusal has its words in Spanish", () => {
  assert.equal(voiceFailureText({ kind: "busy" }), "Ya se está transcribiendo otra nota, prueba en 2 s");
  assert.equal(voiceFailureText({ kind: "forbidden" }), "Este dispositivo no tiene permiso para dictar");
  assert.equal(voiceFailureText({ kind: "unavailable" }), "El dictado no está disponible en el PC");
  assert.match(voiceFailureText({ kind: "tooLarge" }), /60 s/);
  assert.match(voiceFailureText({ kind: "offline" }), /Sin conexión/);
  assert.match(voiceFailureText({ kind: "protocol" }), /conexión/);
  assert.match(voiceFailureText({ kind: "rejected", status: 400 }), /no pudo leer/);
  assert.match(voiceFailureText({ kind: "unknown" }), /Vuelve a intentarlo/);
});

test("the empty transcript has its own words", () => {
  assert.equal(EMPTY_TEXT, "No se oyó nada");
});

test("a microphone refusal names what to do", () => {
  assert.match(micFailureText("NotAllowedError"), /Micrófono/);
  assert.match(micFailureText("NotFoundError"), /ningún micrófono/);
  assert.equal(micFailureText("Otra"), "No se pudo abrir el micrófono.");
});

test("the shortcut is Ctrl+Shift+M and nothing else", () => {
  assert.equal(isDictationShortcut({ ctrlKey: true, shiftKey: true, code: "KeyM" }), true);
  assert.equal(isDictationShortcut({ ctrlKey: true, shiftKey: false, code: "KeyM" }), false);
  assert.equal(isDictationShortcut({ ctrlKey: true, shiftKey: true, altKey: true, code: "KeyM" }), false);
  assert.equal(isDictationShortcut({ ctrlKey: true, shiftKey: true, code: "KeyN" }), false);
});
