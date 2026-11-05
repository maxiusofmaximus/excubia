// Run with: node --test tooling/calendar-ics-model.test.mjs
// «Calendario» en .ics: las palabras del resultado de importar, los motivos de cada omitido
// y el texto de cada refusal de importar o exportar.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  icsErrorText,
  importSummary,
  reasonText,
  savedText,
  skippedLines,
  skippedNote,
} from "../ui/calendar-ics-model.js";

const result = (overrides) => ({ imported: 0, skipped_existing: 0, skipped_unsupported: 0, details: [], ...overrides });

test("cada motivo de omitido tiene sus palabras, y uno desconocido se lee igual", () => {
  assert.equal(reasonText("exists"), "Ya existía");
  assert.equal(reasonText("unsupported_recurrence"), "Repetición no soportada");
  assert.equal(reasonText("exception_instance"), "Ocurrencia suelta de una serie");
  assert.equal(reasonText("no_start"), "Sin fecha de inicio");
  assert.equal(reasonText("invalid_time"), "Hora no válida");
  assert.equal(reasonText("invalid_text"), "Texto no válido");
  assert.equal(reasonText("invalid_recurrence"), "Repetición no válida");
  assert.equal(reasonText("calendar_limit"), "Límite de eventos alcanzado");
  assert.equal(reasonText("something_new"), "Omitido por el PC");
});

test("el resumen cuenta importados, ya existían y omitidos, con el plural de cada cifra", () => {
  assert.equal(
    importSummary(result({ imported: 2, skipped_existing: 1, skipped_unsupported: 1 })),
    "2 importados · 1 ya existía · 1 omitido",
  );
  assert.equal(
    importSummary(result({ imported: 0, skipped_existing: 3, skipped_unsupported: 0 })),
    "0 importados · 3 ya existían · 0 omitidos",
  );
});

test("los omitidos se listan por su puesto en el archivo, desde el uno", () => {
  const lines = skippedLines(
    result({
      details: [
        { index: 0, reason: "exists" },
        { index: 2, reason: "unsupported_recurrence" },
      ],
    }),
  );
  assert.deepEqual(lines, ["Evento 1: Ya existía", "Evento 3: Repetición no soportada"]);
});

test("una lista completa no lleva nota; los omitidos sin detalle se cuentan en una", () => {
  assert.equal(
    skippedNote(
      result({
        skipped_existing: 1,
        skipped_unsupported: 1,
        details: [
          { index: 0, reason: "exists" },
          { index: 1, reason: "no_start" },
        ],
      }),
    ),
    null,
  );
  const long = Array.from({ length: 50 }, (_, index) => ({ index, reason: "no_start" }));
  assert.equal(skippedNote(result({ skipped_unsupported: 52, details: long })), "Hay 2 más sin detalle.");
});

test("lo guardado se anuncia con su nombre y sin ruta", () => {
  assert.equal(savedText("excubia.ics"), "Guardado «excubia.ics».");
});

test("cada refusal de importar o exportar se lee con las palabras de la persona", () => {
  const agenda = (code) => ({ kind: "agendaRefused", code, field: null, message: "" });
  assert.equal(icsErrorText({ kind: "fileRefused", code: "too_large" }, "import"), "El archivo es demasiado grande (máximo 512 KiB).");
  assert.equal(icsErrorText({ kind: "fileRefused", code: "read_failed" }, "import"), "No se pudo leer el archivo.");
  assert.equal(icsErrorText({ kind: "fileRefused", code: "write_failed" }, "export"), "No se pudo guardar el archivo.");
  assert.equal(icsErrorText({ kind: "host", status: 413, message: "" }, "import"), "El archivo es demasiado grande (máximo 512 KiB).");
  assert.equal(icsErrorText(agenda("too_many_events"), "import"), "Tiene más de 500 eventos: divide el archivo.");
  assert.equal(icsErrorText(agenda("too_many_events"), "export"), "Hay demasiados eventos: elige un rango más corto.");
  assert.equal(icsErrorText(agenda("invalid_ics"), "import"), "El archivo no es un calendario .ics válido.");
  assert.equal(
    icsErrorText(agenda("timezone_unset"), "export"),
    "Fija antes la zona horaria de la empresa para importar o exportar.",
  );
  assert.equal(icsErrorText({ kind: "invalid", field: "to", message: "" }, "export"), "El periodo no es válido.");
});
