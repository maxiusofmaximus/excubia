// Run with: node --test tooling/avatar-model.test.mjs
// The avatar's decisions: status → clip, which character a row shows, the initial
// when there is none, which v3 render each size and state takes, Alfred's blink
// interval, and that every file the component can ask for is in ui/assets.
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";

import {
  BLINK_HOLD_MS,
  CHARACTER_IDS,
  avatarSource,
  blinkGapMs,
  characterFor,
  clipFor,
  initialOf,
  renderFile,
  renderSize,
  stateFor,
  stillFor,
} from "../ui/avatar.js";

test("estado → clip, como en la APK: bloqueado es error y lo demás reposo", () => {
  assert.equal(clipFor("working"), "working");
  assert.equal(clipFor("done"), "done");
  assert.equal(clipFor("blocked"), "error");
  assert.equal(clipFor("idle"), "idle");
  assert.equal(clipFor("unknown"), "idle");
  assert.equal(clipFor(undefined), "idle");
});

test("el reposo usa el fotograma neutral; los demás, el suyo", () => {
  assert.equal(stillFor("idle"), "neutral");
  assert.equal(stillFor("error"), "error");
});

test("pensando (Alfred esperando respuesta) manda sobre el estado; sin look, el estado", () => {
  assert.equal(stateFor("working", "thinking"), "thinking");
  assert.equal(stateFor("idle", "thinking"), "thinking");
  assert.equal(stateFor("working", null), "working");
  assert.equal(stateFor("blocked"), "error");
});

test("render de 128 px hasta 64 px de cara y de 384 px por encima", () => {
  assert.equal(renderSize(36), 128);
  assert.equal(renderSize(64), 128);
  assert.equal(renderSize(68), 384);
});

test("hasta 64 px, el retrato de 128; por encima, el de 384", () => {
  assert.equal(avatarSource("bob", "working", 36), "assets/portraits/bob-working-128.webp");
  assert.equal(avatarSource("alfred", "idle", 44), "assets/portraits/alfred-neutral-128.webp");
  assert.equal(avatarSource("alfred", "idle", 68), "assets/portraits/alfred-neutral-384.webp");
  assert.equal(avatarSource("hugo", "done", 128), "assets/portraits/hugo-done-384.webp");
});

test("cada estado tiene su retrato de 384, también working y error", () => {
  assert.equal(renderFile("bob", "done", 200), "assets/portraits/bob-done-384.webp");
  assert.equal(renderFile("alfred", "neutral", 200), "assets/portraits/alfred-neutral-384.webp");
  assert.equal(renderFile("bob", "working", 200), "assets/portraits/bob-working-384.webp");
  assert.equal(renderFile("bob", "error", 200), "assets/portraits/bob-error-384.webp");
});

test("la bienvenida de Alfred es el cuadrado de 384, neutral o parpadeando", () => {
  assert.equal(renderFile("alfred", "neutral", 224), "assets/portraits/alfred-neutral-384.webp");
  assert.equal(renderFile("alfred", "blink", 224), "assets/portraits/alfred-blink-384.webp");
});

test("el id del personaje manda; un id desconocido no se adivina por el nombre", () => {
  assert.equal(characterFor("Zoe", "bob"), "bob");
  assert.equal(characterFor("Bob", "zoe"), null);
});

test("sin id, el personaje sale de un segmento del título, sin distinguir acentos", () => {
  assert.equal(characterFor("Ernesto · dev"), "ernesto");
  assert.equal(characterFor("desconocido · Sami · especialista"), "sami");
  assert.equal(characterFor("Alfred (lead)"), "alfred");
  assert.equal(characterFor("Inés"), "ines");
  assert.equal(characterFor("Ernesto, líder"), "ernesto");
  assert.equal(characterFor("Inesperado"), null);
  assert.equal(characterFor("Zoe"), null);
  assert.equal(characterFor(""), null);
});

test("sin personaje, la inicial en mayúscula; sin nombre, «?»", () => {
  assert.equal(initialOf("zoe"), "Z");
  assert.equal(initialOf("  "), "?");
});

test("el parpadeo dura 120 ms y el intervalo siempre cae entre 3000 y 6000 ms", () => {
  assert.equal(BLINK_HOLD_MS, 120);
  assert.equal(blinkGapMs(() => 0), 3000);
  assert.equal(blinkGapMs(() => 0.9999999), 6000);
  for (let i = 0; i < 1000; i += 1) {
    const gap = blinkGapMs();
    assert.ok(gap >= 3000 && gap <= 6000, `intervalo fuera de rango: ${gap}`);
  }
});

test("todo lo que el componente puede pedir está en ui/assets", () => {
  const states = ["neutral", "working", "done", "error", "thinking"];
  const sizes = [36, 44, 64, 68, 200, 224];
  const paths = [];
  for (const id of CHARACTER_IDS) {
    for (const state of states) for (const px of sizes) paths.push(renderFile(id, state, px));
  }
  // Only Alfred blinks: his shut eyes are the one extra render, at every size.
  for (const px of sizes) paths.push(renderFile("alfred", "blink", px));
  for (const path of paths) {
    assert.ok(existsSync(new URL(`../ui/${path}`, import.meta.url)), `falta ${path}`);
  }
});
