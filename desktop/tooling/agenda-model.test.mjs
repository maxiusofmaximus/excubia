// Run with: node --test tooling/agenda-model.test.mjs
// «Contactos» and «Notas»: the words of each refusal, the local limits, the
// relative date, the origin of an address and the title of a note without subject.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  ADDRESS_PATTERN,
  CONTACT_LIMITS,
  NOTE_LIMITS,
  agendaErrorText,
  contactPrimaryAddress,
  contactProblem,
  noteProblem,
  noteSummary,
  noteTitle,
  parseList,
  relativeDate,
  sourceLabel,
} from "../ui/agenda-model.js";

const NOW = Date.parse("2026-10-29T12:00:00.000Z");
const ago = (ms) => new Date(NOW - ms).toISOString();
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

test("the origin of an address reads as the word the operator knows", () => {
  assert.equal(sourceLabel("phone"), "del teléfono");
  assert.equal(sourceLabel("user_message"), "dictada");
  assert.equal(sourceLabel("alfred"), "Alfred");
  assert.equal(sourceLabel("app"), "creada aquí");
  assert.equal(sourceLabel("otra"), "");
});

test("a relative date says how long ago, and falls back to the day", () => {
  assert.equal(relativeDate(ago(10_000), NOW), "ahora");
  assert.equal(relativeDate(ago(5 * MINUTE), NOW), "hace 5 min");
  assert.equal(relativeDate(ago(2 * HOUR), NOW), "hace 2 h");
  assert.equal(relativeDate(ago(DAY + HOUR), NOW), "ayer");
  assert.equal(relativeDate(ago(3 * DAY), NOW), "hace 3 días");
  const old = ago(40 * DAY);
  assert.equal(
    relativeDate(old, NOW),
    new Date(old).toLocaleDateString("es-ES", { day: "numeric", month: "short" }),
  );
  assert.equal(relativeDate("no es una fecha", NOW), "");
});

test("a note without subject is titled by its first words", () => {
  assert.equal(noteTitle({ subject: "Cliente", excerpt: "x" }), "Cliente");
  assert.equal(noteTitle({ subject: "  ", excerpt: "presupuesto 1.250 € para Ana" }), "presupuesto 1.250 € para Ana");
  assert.equal(
    noteTitle({ subject: null, excerpt: "uno dos tres cuatro cinco seis siete ocho nueve" }),
    "uno dos tres cuatro cinco seis siete ocho…",
  );
  assert.equal(noteTitle({ subject: null, excerpt: "" }), "Nota sin texto");
});

test("a note read or written turns into the list row the host would send", () => {
  const note = { id: "nt_1", subject: null, body: "presupuesto\n\n 1.250 €", updated_at: "2026-10-29T09:00:00.000Z" };
  assert.deepEqual(noteSummary(note), {
    id: "nt_1",
    subject: null,
    excerpt: "presupuesto 1.250 €",
    updated_at: "2026-10-29T09:00:00.000Z",
  });
  assert.equal(noteSummary({ ...note, body: "x".repeat(300) }).excerpt.length, 200);
});

test("the primary address is the first one, or empty", () => {
  assert.equal(contactPrimaryAddress({ addresses: [{ address: "ana@x.es", source: "app" }] }), "ana@x.es");
  assert.equal(contactPrimaryAddress({ addresses: [] }), "");
});

test("a list typed by hand splits on commas and new lines and drops the empty ones", () => {
  assert.deepEqual(parseList(" a@x.es, b@x.es\n\n c@x.es ,"), ["a@x.es", "b@x.es", "c@x.es"]);
  assert.deepEqual(parseList(""), []);
});

test("an address must be local@domain.tld without spaces", () => {
  assert.ok(ADDRESS_PATTERN.test("ana@empresa.es"));
  assert.ok(!ADDRESS_PATTERN.test("ana@empresa"));
  assert.ok(!ADDRESS_PATTERN.test("ana @empresa.es"));
  assert.ok(!ADDRESS_PATTERN.test("ana"));
});

test("a contact needs a name and stays inside the host limits", () => {
  const ok = { name: "Ana", aliases: [], addresses: ["ana@x.es"], notes: "" };
  assert.equal(contactProblem(ok), null);
  assert.equal(contactProblem({ ...ok, name: "  " }).field, "name");
  assert.equal(contactProblem({ ...ok, name: "a".repeat(CONTACT_LIMITS.name + 1) }).field, "name");
  assert.equal(contactProblem({ ...ok, name: "a".repeat(CONTACT_LIMITS.name) }), null);
  const aliases = Array.from({ length: CONTACT_LIMITS.aliases + 1 }, (_, i) => `alias${i}`);
  assert.equal(contactProblem({ ...ok, aliases }).field, "aliases");
  const addresses = Array.from({ length: CONTACT_LIMITS.addresses + 1 }, (_, i) => `a${i}@x.es`);
  assert.equal(contactProblem({ ...ok, addresses }).field, "addresses");
  assert.equal(contactProblem({ ...ok, addresses: ["mal formada"] }).field, "addresses");
});

test("a note needs text, a subject of 120 characters at most and 32 KiB of body", () => {
  assert.equal(noteProblem({ title: null, body: "  " }).field, "body");
  assert.equal(noteProblem({ title: null, body: "hola" }), null);
  assert.equal(noteProblem({ title: "a".repeat(NOTE_LIMITS.subject + 1), body: "x" }).field, "title");
  assert.equal(noteProblem({ title: "a".repeat(NOTE_LIMITS.subject), body: "x" }), null);
  const tooBig = "ñ".repeat(NOTE_LIMITS.bodyBytes / 2 + 1);
  assert.equal(noteProblem({ title: null, body: tooBig }).field, "body");
});

test("every refusal of the agenda has its Spanish words", () => {
  const refusal = (code, field, message = "") => ({ kind: "agendaRefused", code, field, message });
  assert.equal(
    agendaErrorText(refusal("address_taken", "addresses", "the address already belongs to the contact Ana Ruiz")),
    "Esa dirección ya es de «Ana Ruiz». Quítala de ese contacto o usa otra.",
  );
  assert.equal(
    agendaErrorText(refusal("address_taken", "addresses", "sin nombre")),
    "Esa dirección ya pertenece a otro contacto.",
  );
  assert.match(agendaErrorText(refusal("alias_taken")), /alias/);
  assert.match(agendaErrorText(refusal("contact_limit")), /1000 contactos/);
  assert.match(agendaErrorText(refusal("note_limit")), /2000 notas/);
  assert.match(agendaErrorText(refusal("invalid_address", "addresses")), /nombre@dominio/);
  assert.match(agendaErrorText(refusal("empty_message", "body")), /vacía/);
  assert.match(agendaErrorText(refusal("invalid_body", "body")), /32 KiB/);
  assert.match(agendaErrorText(refusal("invalid_arguments", "title")), /asunto/);
  assert.match(agendaErrorText(refusal("invalid_arguments", "name")), /nombre/);
  assert.match(agendaErrorText(refusal("not_found")), /ya no existe/);
  assert.match(agendaErrorText(refusal("unknown_code")), /no aceptó/);
});

test("an offline or unknown failure never shows the raw code", () => {
  assert.match(agendaErrorText({ kind: "offline", message: "x" }), /Sin conexión/);
  assert.match(agendaErrorText({ kind: "host", message: "El host rechazó la petición: x" }), /no aceptó/);
  assert.equal(agendaErrorText(null), "No se pudo cargar. Inténtalo de nuevo.");
});
