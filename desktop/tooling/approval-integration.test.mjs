// Regression for the merge of the Spanish desk fixes with the English strings: the same card
// rules must read the same in both languages. Run with: node --test tooling/approval-integration.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { setLang } from "../ui/i18n.js";
import { detailFacts, humanHeadline, settledLine } from "../ui/approval-model.js";

const shell = (tool, headline, extra = {}) => ({ tool, agent_label: "Ernesto", headline, summary: "", ...extra });

test("ES: a shell command says «ejecutar un comando», a Write keeps «guardar», a known delete or read names its file", () => {
  setLang("es");
  const psCommand = { payload_redacted: { command: "Set-Content -Path C:\\p\\comando-p8.txt -Value hola" } };
  const ps = shell("PowerShell", "create comando-p8.txt with text hola", { paths: ["C:/p/comando-p8.txt"], ...psCommand });
  assert.equal(humanHeadline(ps), "Ernesto quiere ejecutar un comando");
  assert.equal(settledLine("approved", { ...ps, scope: "once" }), "✓ Permitido · ejecutar un comando");
  assert.deepEqual(detailFacts(ps).technical[0], ["Comando", "Set-Content -Path C:\\p\\comando-p8.txt -Value hola"]);
  assert.equal(humanHeadline({ tool: "Write", agent_label: "Ernesto", paths: ["C:/p/uno.txt"] }), "Ernesto quiere guardar «uno.txt»");
  assert.equal(humanHeadline(shell("Bash", "delete old notes", { paths: ["C:/p/a.txt"] })), "Ernesto quiere borrar «a.txt»");
  assert.equal(humanHeadline(shell("Bash", "remove the cache")), "Ernesto quiere borrar archivos");
  assert.equal(humanHeadline({ tool: "Read", agent_label: "Ernesto", paths: ["C:/p/a.txt"] }), "Ernesto quiere leer «a.txt»");
});

test("EN: the same rules in English; the command stays under the technical details", () => {
  setLang("en");
  try {
    const ps = shell("PowerShell", "create comando-p8.txt with text hola", { paths: ["C:/p/comando-p8.txt"], payload_redacted: { command: "Set-Content -Path a -Value b" } });
    assert.equal(humanHeadline(ps), "Ernesto wants to run a command");
    assert.equal(settledLine("approved", { ...ps, scope: "once" }), "✓ Allowed · run a command");
    assert.deepEqual(detailFacts(ps).technical[0], ["Command", "Set-Content -Path a -Value b"]);
    assert.equal(humanHeadline({ tool: "Write", agent_label: "Ernesto", paths: ["C:/p/uno.txt"] }), "Ernesto wants to save “uno.txt”");
    assert.equal(humanHeadline(shell("Bash", "delete old notes", { paths: ["C:/p/a.txt"] })), "Ernesto wants to delete “a.txt”");
    assert.equal(humanHeadline(shell("Bash", "remove the cache")), "Ernesto wants to delete files");
    assert.equal(humanHeadline({ tool: "Read", agent_label: "Ernesto", paths: ["C:/p/a.txt"] }), "Ernesto wants to read “a.txt”");
  } finally {
    setLang("es");
  }
});
