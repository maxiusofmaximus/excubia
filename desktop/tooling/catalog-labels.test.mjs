// Run with: node --test tooling/catalog-labels.test.mjs
// The team_roles catalogue's areas and role titles, read by their stable ids: English translates,
// Spanish keeps the host's text, an unknown id keeps the label it came with, and the input is never changed.
import { test } from "node:test";
import assert from "node:assert/strict";

import { areaLabel, roleLabel } from "../ui/catalog-labels.js";
import { dictionaries, setLang } from "../ui/i18n.js";

// Strict-mode writes to a frozen object throw, so a helper that mutated its argument would fail here.
const deepFreeze = (value) => {
  for (const child of Object.values(value)) if (child && typeof child === "object") deepFreeze(child);
  return Object.freeze(value);
};

test("en: un área y un oficio del catálogo integrado se traducen por su ID", () => {
  try {
    setLang("en");
    assert.equal(areaLabel({ id: "programacion", label: "Programación" }), "Programming");
    assert.equal(areaLabel({ id: "otras", label: "Otras (educación, legal, finanzas)" }), "Other (education, legal, finance)");
    assert.equal(roleLabel({ id: "bob", name: "Bob", title: "Desarrollo de software" }), "Software development");
    assert.equal(roleLabel({ id: "maya", name: "Maya", title: "Especialista SEO" }), "SEO specialist");
  } finally {
    setLang("es");
  }
});

test("es: se muestra el texto que entrega el host, aunque sea distinto al del diccionario", () => {
  try {
    setLang("es");
    assert.equal(areaLabel({ id: "video", label: "Vídeo y postproducción" }), "Vídeo y postproducción");
    assert.equal(roleLabel({ id: "vera", title: "Edición de vídeo" }), "Edición de vídeo");
    assert.equal(roleLabel({ id: "bob", title: "Otro texto del host" }), "Otro texto del host");
  } finally {
    setLang("es");
  }
});

test("un área o un oficio desconocido conserva la etiqueta original, en cualquier idioma", () => {
  try {
    for (const language of ["en", "es"]) {
      setLang(language);
      assert.equal(areaLabel({ id: "area_nueva", label: "Área nueva" }), "Área nueva");
      assert.equal(roleLabel({ id: "oficio_nuevo", title: "Oficio nuevo" }), "Oficio nuevo");
      assert.equal(roleLabel({ title: "Sin ID" }), "Sin ID");
      assert.equal(roleLabel(undefined), undefined);
      assert.equal(areaLabel(undefined), undefined);
    }
  } finally {
    setLang("es");
  }
});

test("no muta los datos de entrada, ni siquiera al traducir", () => {
  try {
    setLang("en");
    const area = deepFreeze({ id: "marketing", label: "Marketing y contenido", roles: ["aisha", "maya"] });
    const role = deepFreeze({ id: "maya", name: "Maya", title: "Especialista SEO", avatar: "maya" });
    const before = structuredClone({ area, role });
    assert.equal(areaLabel(area), "Marketing and content");
    assert.equal(roleLabel(role), "SEO specialist");
    assert.deepEqual({ area, role }, before);
    assert.equal(role.name, "Maya");
  } finally {
    setLang("es");
  }
});

test("el catálogo integrado tiene 9 áreas y 43 oficios con su traducción en inglés", () => {
  const keys = Object.keys(dictionaries.es).filter((key) => key.startsWith("catalog."));
  assert.equal(keys.filter((key) => key.startsWith("catalog.area.")).length, 9);
  assert.equal(keys.filter((key) => key.startsWith("catalog.role.")).length, 43);
  for (const key of keys) assert.ok(Object.hasOwn(dictionaries.en, key), `${key} sin inglés`);
});
