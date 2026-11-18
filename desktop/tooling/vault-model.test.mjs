// Run with: node --test tooling/vault-model.test.mjs
// «Protege tu historial»: the packed mouse samples, the pool's count and the recovery challenge.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  SAMPLES_MAX,
  SAMPLES_NEEDED,
  checkChallenge,
  confirmChallenge,
  createCollector,
  finishErrorText,
  protectErrorText,
} from "../ui/vault-model.js";

const PHRASE = Array.from({ length: 24 }, (_, index) => `palabra${index + 1}`).join(" ");

// Decodes the packed pool the way the host reads it, for the round trip.
function unpack(base64) {
  const bytes = Buffer.from(base64, "base64");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const records = [];
  for (let at = 0; at < bytes.length; at += 16) {
    records.push({
      x: view.getInt32(at, true),
      y: view.getInt32(at + 4, true),
      micros: view.getBigUint64(at + 8, true),
    });
  }
  return records;
}

test("each sample packs to exactly 16 bytes and decodes to the same values", () => {
  const collector = createCollector();
  collector.add(10, -20, 1_500_000);
  collector.add(-7, 2147483647, 42);
  collector.add(3, 4, 2 ** 40 + 5);

  const packed = collector.pack();
  assert.equal(Buffer.from(packed, "base64").length, 3 * 16);
  assert.deepEqual(unpack(packed), [
    { x: 10, y: -20, micros: 1_500_000n },
    { x: -7, y: 2147483647, micros: 42n },
    { x: 3, y: 4, micros: 2n ** 40n + 5n },
  ]);
});

test("a sample equal to the previous one in x and y does not count", () => {
  const collector = createCollector(3);
  collector.add(5, 5, 1);
  collector.add(5, 5, 2);
  collector.add(5, 5, 3);
  assert.equal(collector.count(), 1);
  collector.add(6, 5, 4);
  assert.equal(collector.count(), 2);
});

test("enough means 2048 samples, and progress stops at 1", () => {
  const collector = createCollector();
  assert.equal(SAMPLES_NEEDED, 2048);
  for (let i = 0; i < SAMPLES_NEEDED - 1; i += 1) collector.add(i, 0, i);
  assert.equal(collector.isEnough(), false);
  assert.equal(collector.progress(), (SAMPLES_NEEDED - 1) / SAMPLES_NEEDED);
  collector.add(-1, 0, 0);
  assert.equal(collector.isEnough(), true);
  assert.equal(collector.progress(), 1);
});

test("the pool stops growing at its maximum and stays under the route limit", () => {
  const collector = createCollector();
  for (let i = 0; i < SAMPLES_MAX + 50; i += 1) collector.add(i, i % 2, i);
  assert.equal(collector.count(), SAMPLES_MAX);
  assert.ok(collector.pack().length + 14 < 64 * 1024);
});

test("clear drops the samples", () => {
  const collector = createCollector();
  collector.add(1, 1, 1);
  collector.clear();
  assert.equal(collector.count(), 0);
  assert.equal(collector.pack(), "");
});

test("the challenge picks three different positions from 1 to 24, in order", () => {
  for (let seed = 0; seed < 500; seed += 1) {
    const positions = confirmChallenge(PHRASE, seed);
    assert.equal(positions.length, 3);
    assert.equal(new Set(positions).size, 3);
    for (const position of positions) assert.ok(position >= 1 && position <= 24, `${position}`);
    assert.deepEqual([...positions].sort((a, b) => a - b), positions);
  }
  assert.deepEqual(confirmChallenge(PHRASE, 7), confirmChallenge(PHRASE, 7));
});

test("the challenge refuses a phrase that is not 24 words", () => {
  assert.throws(() => confirmChallenge("solo tres palabras"), /24 palabras/);
});

test("the challenge accepts the right words and rejects the wrong ones", () => {
  const positions = [2, 9, 24];
  assert.equal(checkChallenge(PHRASE, positions, ["palabra2", "palabra9", "palabra24"]), true);
  assert.equal(checkChallenge(PHRASE, positions, ["  PALABRA2 ", "Palabra9", " palabra24"]), true);
  assert.equal(checkChallenge(PHRASE, positions, ["palabra2", "palabra9", "palabra23"]), false);
  assert.equal(checkChallenge(PHRASE, positions, ["palabra2", "palabra9"]), false);
  assert.equal(checkChallenge(PHRASE, positions, ["", "", ""]), false);
  assert.equal(checkChallenge(PHRASE, [1, 2, 25], ["palabra1", "palabra2", "x"]), false);
});

test("the protect dialog words its refusals, and only those two codes get their own text", () => {
  assert.equal(protectErrorText({ kind: "vaultRefused", code: "vault_samples_insufficient" }), "Faltó movimiento, inténtalo otra vez.");
  assert.equal(protectErrorText({ kind: "vaultRefused", code: "vault_already_protected" }), "Tu historial ya está cifrado.");
  const generic = protectErrorText({ kind: "vaultRefused", code: "vault_migration_failed" });
  assert.equal(generic, "No se pudo proteger el historial; no se cambió nada. Reinicia Excubia y vuelve a intentarlo.");
  assert.equal(protectErrorText(undefined), generic);
});

test("the removal of the backup words the missing copy and the rest", () => {
  assert.equal(finishErrorText({ kind: "vaultRefused", code: "vault_no_backup" }), "Ya no queda copia sin cifrar.");
  assert.equal(finishErrorText(new Error("sin red")), "No se pudo borrar la copia. Inténtalo otra vez.");
});
