// Run with: node --test tooling/avatar-model.test.mjs
// The avatar's decisions: status → clip, which character a row shows, the initial
// when there is none, the still under reduced motion, and that every file the
// component can ask for was synced into ui/assets.
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";

import {
  CHARACTER_IDS,
  avatarSource,
  characterFor,
  clipFor,
  initialOf,
  renderSize,
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

test("render de 64 px hasta 64 px de cara y de 128 px por encima", () => {
  assert.equal(renderSize(36), 64);
  assert.equal(renderSize(64), 64);
  assert.equal(renderSize(68), 128);
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
  assert.equal(characterFor("Zoe"), null);
  assert.equal(characterFor(""), null);
});

test("sin personaje, la inicial en mayúscula; sin nombre, «?»", () => {
  assert.equal(initialOf("zoe"), "Z");
  assert.equal(initialOf("  "), "?");
});

test("con movimiento reducido se muestra el fotograma estático", () => {
  assert.equal(
    avatarSource("bob", "working", 36, false),
    "assets/agents_anim/bob/working-head-64.webp",
  );
  assert.equal(avatarSource("bob", "working", 36, true), "assets/agents/bob-working-64.png");
  assert.equal(avatarSource("alfred", "idle", 68, true), "assets/agents/alfred-neutral-128.png");
});

test("todo lo que el componente puede pedir está sincronizado en ui/assets", () => {
  for (const id of CHARACTER_IDS) {
    for (const status of ["idle", "working", "done", "blocked"]) {
      for (const px of [36, 68]) {
        for (const reduced of [false, true]) {
          const path = avatarSource(id, status, px, reduced);
          assert.ok(existsSync(new URL(`../ui/${path}`, import.meta.url)), `falta ${path}`);
        }
      }
    }
  }
});
