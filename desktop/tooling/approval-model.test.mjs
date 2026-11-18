// Run with: node --test tooling/approval-model.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  canApproveMove,
  canAlwaysAllow,
  canApproveTask,
  cardCommand,
  commandText,
  conversationContext,
  conversationProject,
  countdownText,
  cwdLabel,
  folderName,
  isLookCommand,
  repeatsCard,
  riskBadge,
  decisionArgs,
  denyReason,
  destLabel,
  detailFacts,
  excludeFor,
  grantCommand,
  humanHeadline,
  isChecking,
  isMailMove,
  isMailOrAgendaTool,
  isMoveBatch,
  isOperatorLevel,
  isPendingHere,
  isPolicyLine,
  levelChain,
  isWaitingLine,
  lineSummary,
  localFor,
  rememberAction,
  moveApproveLabel,
  moveCounts,
  moveItems,
  REASON_MAX,
  resultText,
  riskNote,
  sameDecision,
  settledLine,
  triageLabel,
  moveTitle,
  patchLocal,
  remainingSeconds,
  sameIds,
  seatName,
  setBootstrap,
  stripBelongs,
  shortDigest,
  statusMeta,
  subjectText,
  supersededIndexes,
} from "../ui/approval-model.js";

test("the chain stops at the level that holds the approval now", () => {
  assert.deepEqual(levelChain("lead"), ["policy", "scorer", "lead"]);
  assert.deepEqual(levelChain("user"), ["policy", "scorer", "lead", "cto", "user"]);
  assert.deepEqual(levelChain("unknown"), []);
});

test("only the operator's level earns a card", () => {
  assert.equal(isOperatorLevel({ level: "user" }), true);
  assert.equal(isOperatorLevel({ level: "lead" }), false);
  assert.equal(isOperatorLevel({}), false);
});

test("the countdown never goes below zero and ignores a bad date", () => {
  const now = Date.parse("2026-10-23T10:00:00.000Z");
  assert.equal(remainingSeconds("2026-10-23T10:02:00.000Z", now), 120);
  assert.equal(remainingSeconds("2026-10-23T09:59:00.000Z", now), 0);
  assert.equal(remainingSeconds("no es una fecha", now), null);
});

test("the countdown reads minutes and padded seconds", () => {
  assert.equal(countdownText(120), "2:00");
  assert.equal(countdownText(65), "1:05");
  assert.equal(countdownText(9), "0:09");
});

test("a Windows cwd reads as its last folder", () => {
  assert.equal(cwdLabel("D:\\proyectos\\demo"), "demo/…");
  assert.equal(cwdLabel("/home/max/app/"), "app/…");
  assert.equal(cwdLabel(""), "");
});

test("the digest is shortened but keeps its algorithm prefix", () => {
  assert.equal(shortDigest("sha256:9f86d0aabbccddeeff00"), "sha256:9f86d0a…");
  assert.equal(shortDigest("sha256:abc"), "sha256:abc");
  assert.equal(shortDigest(undefined), "");
});

test("the agent name comes from the seat id when the host sends none", () => {
  assert.equal(seatName({ seat_id: "ag_bob" }), "Bob");
  assert.equal(seatName({ seat_id: "ag_bob", agent_label: "Roberto" }), "Roberto");
  assert.equal(seatName({}), "Un agente");
  assert.equal(seatName({ agent_label: "01M5MA7C4KBNJ1H4Q6RSJHKVYP" }), "Un agente");
  assert.equal(seatName({ seat_id: "ag_01M5MA7C4KBNJ1H4Q6RSJHKVYP" }), "Un agente");
});

test("the canonical grant command names the device and the permission", () => {
  assert.equal(grantCommand("dev_01"), "host devices grant dev_01 --permission approve");
  assert.equal(grantCommand(null), "host devices grant <dispositivo> --permission approve");
});

test("the command comes from the redacted payload before the summary", () => {
  assert.equal(commandText({ payload_redacted: { command: "cargo test" }, summary: "x" }), "cargo test");
  assert.equal(commandText({ summary: "Ejecutar tests" }), "Ejecutar tests");
});

test("an unknown status gets a neutral label instead of a crash", () => {
  assert.deepEqual(statusMeta("weird"), { label: "weird", tone: "unknown" });
  assert.equal(statusMeta("expired").tone, "expired");
});

test("local facts are kept per approval and merged, not replaced", () => {
  patchLocal("apr_a", { clientId: "c1" });
  patchLocal("apr_a", { busy: true });
  assert.deepEqual(localFor("apr_a"), { clientId: "c1", busy: true });
  assert.deepEqual(localFor("apr_missing"), {});
});

test("one approval keeps only its newest row in the feed", () => {
  const entries = [
    { kind: "message", message: {} },
    { kind: "approval", entry: { ref_id: "apr_1", status: "pending" } },
    { kind: "approval", entry: { ref_id: "apr_2", status: "pending" } },
    { kind: "approval", entry: { ref_id: "apr_1", status: "approved" } },
  ];
  assert.deepEqual([...supersededIndexes(entries)], [1]);
});

test("an approval with a card is not drawn again as a timeline line", () => {
  const entries = [
    { kind: "approval", entry: { ref_id: "apr_1", status: "approved" } },
    { kind: "approval", entry: { ref_id: "apr_2", status: "approved" } },
  ];
  assert.deepEqual([...supersededIndexes(entries, new Set(["apr_1"]))], [0]);
});

test("the bootstrap names the seats and the conversations the operator reads", () => {
  setBootstrap({
    bootstrap: {
      alfred: { agent_id: "01M5MA7C4KBNJ1H4Q6RSJHKVYP", name: "Alfred" },
      projects: [{ id: "prj_1", name: "Demo", lead: { agent_id: "ag_lead", name: "Ernesto" } }],
    },
    conversations: [{ id: "cnv_1", title: "Plan", project_id: "prj_1" }],
  });
  assert.equal(seatName({ seat_id: "01M5MA7C4KBNJ1H4Q6RSJHKVYP" }), "Alfred");
  assert.equal(seatName({ seat_id: "ag_lead" }), "Ernesto");
  assert.equal(conversationContext("cnv_1"), "Demo · Plan");
  assert.equal(conversationContext("cnv_missing"), "");
  assert.equal(conversationProject("cnv_1"), "Demo");
  assert.equal(conversationProject("cnv_missing"), "");
  setBootstrap(null);
  assert.equal(seatName({ seat_id: "ag_lead" }), "Lead");
  assert.equal(conversationProject("cnv_1"), "");
});

test("the headline says what the agent wants in plain words, and never the raw command", () => {
  const bob = { seat_id: "ag_bob" };
  assert.equal(
    humanHeadline({ ...bob, tool: "Write", paths: ["D:\\obra\\uno-t.txt"] }),
    "Bob quiere guardar «uno-t.txt»",
  );
  assert.equal(
    humanHeadline({ ...bob, tool: "Edit", paths: ["/home/max/app/src/main.rs"] }),
    "Bob quiere guardar «main.rs»",
  );
  assert.equal(humanHeadline({ ...bob, tool: "Write" }), "Bob quiere guardar un archivo");
  assert.equal(
    humanHeadline({ ...bob, tool: "Bash", paths: ["D:\\x\\README.md"], payload_redacted: { command: "rm -rf README.md" } }),
    "Bob quiere borrar «README.md»",
  );
  assert.equal(
    humanHeadline({ ...bob, tool: "PowerShell", payload_redacted: { command: "Remove-Item .\\a.txt" } }),
    "Bob quiere borrar «a.txt»",
  );
  assert.equal(
    humanHeadline({ ...bob, tool: "Bash", payload_redacted: { command: "pwd && ls -la" } }),
    "Bob quiere mirar el proyecto",
  );
  assert.equal(
    humanHeadline({ ...bob, tool: "Bash", payload_redacted: { command: "ls -la && rm README.md" } }),
    "Bob quiere borrar «README.md»",
  );
  assert.equal(humanHeadline({ ...bob, tool: "Read", paths: ["notas/x"] }), "Bob quiere leer «x»");
  assert.equal(humanHeadline({ ...bob, tool: "Read" }), "Bob quiere leer un archivo");
  assert.equal(humanHeadline({ ...bob, tool: "Grep" }), "Bob quiere usar una herramienta");
  assert.equal(humanHeadline({ tool: "Bash" }), "Un agente quiere ejecutar un comando");
});

test("a simple delete names its files from the command: two at most, then how many more", () => {
  const bob = { seat_id: "ag_bob", tool: "Bash" };
  const named = (command) => humanHeadline({ ...bob, payload_redacted: { command } });
  assert.equal(named("rm README.md"), "Bob quiere borrar «README.md»");
  assert.equal(named("rm -f a.txt b.txt"), "Bob quiere borrar «a.txt» y «b.txt»");
  assert.equal(named("rm a.txt b.txt c.txt d.txt"), "Bob quiere borrar «a.txt», «b.txt» y 2 más");
  assert.equal(named("del D:\\obra\\uno-t.txt"), "Bob quiere borrar «uno-t.txt»");
  assert.equal(named("unlink ./a.txt"), "Bob quiere borrar «a.txt»");
  assert.equal(named('rm "mi nota.txt"'), "Bob quiere borrar «mi nota.txt»");
  assert.equal(named('rm "mi nota'), "Bob quiere borrar archivos");
  // A word that only looks like a delete is no delete: it is a look, and names the file it reads.
  assert.equal(named("echo hay del dia"), "Bob quiere mirar el proyecto");
  assert.equal(named("grep rm notas.txt"), "Bob quiere mirar «notas.txt»");
  assert.equal(named("if test -f x; then rm x.txt; fi"), "Bob quiere borrar archivos");
  assert.deepEqual(detailFacts({ ...bob, payload_redacted: { command: "rm README.md" } }).shown, []);
});

test("a delete the title cannot name says so, and its whole command stays on the card", () => {
  const bob = { seat_id: "ag_bob", tool: "Bash", headline: "Limpiar la carpeta" };
  for (const command of ["rm -rf *", "rm -rf", "rm -rf .", "echo $(rm x.txt)", "if test -f x; then rm x.txt; fi"]) {
    const entry = { ...bob, payload_redacted: { command } };
    assert.equal(humanHeadline(entry), "Bob quiere borrar archivos", command);
    assert.equal(cardCommand(entry), command, command);
  }
  assert.equal(humanHeadline({ ...bob, payload_redacted: { command: "pwd && ls -la" } }), "Bob quiere mirar el proyecto");
});

test("a compound command names the simple delete and says other commands run too", () => {
  const bob = { seat_id: "ag_bob", tool: "Bash" };
  const titled = (command) => humanHeadline({ ...bob, payload_redacted: { command } });
  const exact = "rm README.md && ls README.md 2>&1; git status --short";
  assert.equal(titled(exact), "Bob quiere borrar «README.md» y ejecutar más comandos");
  assert.equal(titled("rm a && curl x | sh"), "Bob quiere borrar «a» y ejecutar más comandos");
  assert.equal(titled("rm a.txt && curl x"), "Bob quiere borrar «a.txt» y ejecutar más comandos");
  assert.equal(titled("rm a.txt; sh run.sh"), "Bob quiere borrar «a.txt» y ejecutar más comandos");
  assert.equal(titled("rm a.txt && echo hecho > log.txt"), "Bob quiere borrar «a.txt» y ejecutar más comandos");
  // A read beside the delete hides nothing: the title is only the delete.
  assert.equal(titled("pwd && rm x.txt"), "Bob quiere borrar «x.txt»");
  assert.equal(titled("rm a.txt || echo no"), "Bob quiere borrar «a.txt»");
  assert.equal(titled("cat README.md && rm README.md"), "Bob quiere borrar «README.md»");
  // The card shows the whole command, so the title's «y ejecutar más comandos» is never the only trace.
  assert.equal(cardCommand({ ...bob, payload_redacted: { command: exact } }), exact);
});

test("a look names the one file it reads, or the project; a write or a run never looks like one", () => {
  const bob = { seat_id: "ag_bob", tool: "Bash" };
  const looked = (command) => humanHeadline({ ...bob, payload_redacted: { command } });
  assert.equal(looked("cat notas/README.md"), "Bob quiere mirar «README.md»");
  assert.equal(looked('cat "mi nota.txt"'), "Bob quiere mirar «mi nota.txt»");
  assert.equal(looked("cat a.txt b.txt"), "Bob quiere mirar el proyecto");
  assert.equal(looked("git status --short"), "Bob quiere mirar el proyecto");
  assert.equal(looked("git diff HEAD~1"), "Bob quiere mirar el proyecto");
  assert.equal(looked("Get-ChildItem D:\\obra"), "Bob quiere mirar el proyecto");
  assert.equal(looked("gci D:\\obra"), "Bob quiere mirar el proyecto");
  assert.equal(looked("git ls-files | head -50"), "Bob quiere mirar el proyecto");
  assert.equal(looked("git rev-parse HEAD"), "Bob quiere mirar el proyecto");
  assert.equal(looked("ls | grep x"), "Bob quiere mirar el proyecto");
  for (const command of ["git push", "ls > out.txt", "cat x 2>&1 > y", "rg --pre sh x", "git diff --output=x"]) {
    assert.equal(looked(command), "Bob quiere ejecutar un comando", command);
  }
  for (const command of ["ls\nrm x", "ls & rm x", "cat $(rm x)"]) {
    assert.equal(isLookCommand(command), false, command);
  }
  // The host's category wins over the words when it sends one.
  assert.equal(humanHeadline({ ...bob, category: "read", payload_redacted: { command: "make all" } }), "Bob quiere mirar el proyecto");
  assert.equal(humanHeadline({ ...bob, category: "write", payload_redacted: { command: "ls" } }), "Bob quiere ejecutar un comando");
  assert.equal(isLookCommand(""), false);
});

test("a folder reads as its last name; the full path is only in the details", () => {
  assert.equal(folderName("C:\\Users\\Max\\bench-proj"), "bench-proj");
  assert.equal(folderName("/home/max/app/"), "app");
  assert.equal(folderName(""), "");
});

test("the risk badge shows only a medium or high risk, never for a look, never a guessed one", () => {
  assert.equal(riskBadge({ tool: "Bash", risk: "low" }), "");
  assert.equal(riskBadge({ tool: "Bash" }), "");
  assert.equal(riskBadge({ tool: "Bash", risk: "medium" }), "Riesgo medio");
  // The host's risk wins over the words: a look that reaches outside the project keeps its badge.
  assert.equal(riskBadge({ tool: "Bash", risk: "high", payload_redacted: { command: "cat /etc/passwd" } }), "Riesgo alto");
  assert.equal(riskBadge({ tool: "Bash", risk: "low", payload_redacted: { command: "ls -la" } }), "");
  assert.equal(riskBadge({ tool: "Write", risk: "high" }), "Riesgo alto");
  assert.equal(riskNote({ tool: "Bash", risk: "low", payload_redacted: { command: "git status" } }), "");
  assert.equal(riskNote({ tool: "Bash", risk: "high", payload_redacted: { command: "cat /etc/passwd" } }), "Puede cambiar o borrar archivos.");
});

test("a read-only chain on a low risk is a look: its card says so and never says it can change files", () => {
  const ernesto = { tool: "Bash", agent_label: "Ernesto", risk: "low", payload_redacted: { command: "pwd && ls -la && git status --short && git ls-files | head -50" } };
  assert.equal(humanHeadline(ernesto), "Ernesto quiere mirar el proyecto");
  assert.equal(riskNote(ernesto), "");
});

test("a timeline line never shows the tool's name: it names the action its card named", () => {
  // A waiting request has its card; its «Pendiente» line says nothing, so it never claims «Esperando tu OK».
  assert.equal(isWaitingLine("Pendiente: command Bash"), true);
  assert.equal(isWaitingLine("Operador denegó: command Bash"), false);
  assert.equal(lineSummary("Pendiente: command Bash", "apr_line_1"), "");
  assert.equal(lineSummary("Operador denegó: command Bash", "apr_line_1"), "Denegaste: ejecutar un comando");
  rememberAction({ id: "apr_line_2", tool: "Bash", seat_id: "ag_bob", payload_redacted: { command: "rm README.md" } });
  assert.equal(lineSummary("Pendiente: command Bash", "apr_line_2"), "");
  assert.equal(lineSummary("Operador denegó: command Bash", "apr_line_2"), "Denegaste: borrar «README.md»");
  assert.equal(lineSummary("Operador aprobó: command Bash", "apr_line_2"), "Aprobaste: borrar «README.md»");
  assert.equal(lineSummary("Host expiró: command Bash", "apr_line_2"), "Caducó sin respuesta: borrar «README.md»");
  // A sentence the host wrote without a tool name stays as it is.
  assert.equal(lineSummary("Caducó sin respuesta", "apr_line_2"), "Caducó sin respuesta");
  assert.equal(lineSummary("", "apr_line_2"), "");
});

test("a denial its card already settles is not said twice on the timeline", () => {
  const cards = new Set(["apr_1"]);
  assert.equal(repeatsCard({ ref_id: "apr_1", status: "denied" }, cards), true);
  assert.equal(repeatsCard({ ref_id: "apr_1", status: "approved" }, cards), false);
  assert.equal(repeatsCard({ ref_id: "apr_2", status: "denied" }, cards), false);
});

test("the risk is named only for a command that is high-risk", () => {
  assert.equal(riskNote({ tool: "Bash", risk: "high" }), "Puede cambiar o borrar archivos.");
  assert.equal(riskNote({ tool: "Bash", risk: "low" }), "");
  assert.equal(riskNote({ tool: "Write", risk: "high" }), "");
});

test("a decided request leaves one line that says what it was about", () => {
  const write = { tool: "Write", paths: ["D:\\obra\\uno-t.txt"] };
  assert.equal(settledLine("approved", { ...write, scope: "once" }), "✓ Permitido · guardar «uno-t.txt»");
  assert.equal(settledLine("consumed", { ...write, scope: "once" }), "✓ Permitido · guardar «uno-t.txt»");
  assert.equal(
    settledLine("approved", { ...write, scope: "task" }),
    "Permitido para toda la tarea · guardar «uno-t.txt»",
  );
  assert.equal(settledLine("denied", write), "✕ Denegado · guardar «uno-t.txt»");
  assert.equal(settledLine("expired", write), "Caducó sin respuesta · guardar «uno-t.txt»");
});

test("mail and agenda approvals keep their own card, every other tool gets the plain one", () => {
  assert.equal(isMailOrAgendaTool({ tool: "mail.send" }), true);
  assert.equal(isMailOrAgendaTool({ tool: "calendar.create" }), true);
  assert.equal(isMailOrAgendaTool({ tool: "Bash" }), false);
  assert.equal(isMailOrAgendaTool({}), false);
});

test("a pending card at zero asks the host; the host still decides", () => {
  const now = Date.parse("2026-10-23T10:00:00.000Z");
  assert.equal(isChecking({ status: "pending", expires_at: "2026-10-23T09:59:59.000Z" }, now), true);
  assert.equal(isChecking({ status: "pending", expires_at: "2026-10-23T10:01:00.000Z" }, now), false);
  assert.equal(isChecking({ status: "expired", expires_at: "2026-10-23T09:59:59.000Z" }, now), false);
});

const batch = {
  tool: "mail.move",
  details: {
    account: "trabajo",
    items: [
      { message_id: "m1", dest: "junkemail", from_address: "ofertas@tienda.invalid", subject: "Oferta", level: "lead" },
      { message_id: "m2", dest: "inbox", subject: "Factura", level: "lead", excluded: true },
      { message_id: "m3", dest: "deleteditems", subject: "Boletín", level: "lead" },
    ],
  },
};

test("a mail.move with mails earns the checklist, anything else does not", () => {
  assert.equal(isMailMove({ tool: "mail.send" }), false);
  assert.equal(isMoveBatch(batch), true);
  assert.equal(isMoveBatch({ tool: "mail.move" }), false);
  assert.equal(isMoveBatch({ tool: "mail.move", details: { items: [] } }), false);
});

test("the batch keeps the host's excluded marks and drops rows that cannot be answered", () => {
  const items = moveItems({
    tool: "mail.move",
    details: { items: [{ message_id: "m1" }, { subject: "sin id" }, null, { message_id: 7 }, { message_id: "m2", excluded: 1 }] },
  });
  assert.deepEqual(items.map((item) => item.message_id), ["m1", "m2"]);
  assert.deepEqual(items.map((item) => item.excluded), [false, true]);
  assert.deepEqual(moveItems({ details: { items: "no es una lista" } }), []);
  assert.deepEqual(moveItems({}), []);
});

test("a destination reads in Spanish and an unknown one never shows raw", () => {
  assert.equal(destLabel("junkemail"), "No deseado");
  assert.equal(destLabel("deleteditems"), "Eliminados");
  assert.equal(destLabel("inbox"), "Bandeja de entrada");
  assert.equal(destLabel("constructor"), "Destino desconocido");
  assert.equal(triageLabel("bulk"), "Masivo");
  assert.equal(triageLabel("lead"), "Sin evaluar");
  assert.equal(triageLabel("constructor"), "Sin evaluar");
});

test("a long subject is trimmed to one short line, and an empty one says so", () => {
  assert.equal(subjectText("  Hola  "), "Hola");
  assert.equal(subjectText(""), "(sin asunto)");
  assert.equal(subjectText(undefined), "(sin asunto)");
  assert.equal(subjectText("x".repeat(100)).length, 80);
  assert.ok(subjectText("x".repeat(100)).endsWith("…"));
});

test("the unchecked mails are the ones the operator leaves out of this batch", () => {
  const unchecked = new Set(["m1", "m9"]);
  assert.deepEqual(excludeFor(batch, unchecked), ["m1"]);
  assert.deepEqual(excludeFor(batch), []);
  assert.deepEqual(excludeFor({}, unchecked), []);
});

test("the count and the label say how many mails stay checked", () => {
  assert.deepEqual(moveCounts(batch, new Set()), { checked: 2, total: 2 });
  assert.deepEqual(moveCounts(batch, new Set(["m1"])), { checked: 1, total: 2 });
  assert.equal(moveApproveLabel(batch, new Set(["m1"])), "Mover los seleccionados (1 de 2)");
  assert.equal(moveTitle(batch), "3 correos a mover");
});

test("the approval cannot go ahead with every mail unchecked", () => {
  assert.equal(canApproveMove(batch, new Set()), true);
  assert.equal(canApproveMove(batch, new Set(["m1", "m3"])), false);
  assert.equal(canApproveMove({ tool: "mail.move", details: { items: [] } }), false);
});

test("a retry reuses its id only for the same list of mails left out", () => {
  assert.equal(sameIds(["m1"], ["m1"]), true);
  assert.equal(sameIds(["m1"], ["m1", "m2"]), false);
  assert.equal(sameIds(undefined, []), true);
});

test("«Siempre en esta tarea» needs an agent's seat and never a mail request", () => {
  assert.equal(canApproveTask({ seat_id: "ag_bob", tool: "bash" }), true);
  assert.equal(canApproveTask({ tool: "bash" }), false);
  assert.equal(canApproveTask({ seat_id: "", tool: "bash" }), false);
  assert.equal(canApproveTask({ seat_id: "ag_bob", tool: "mail.send" }), false);
  assert.equal(canApproveTask({ seat_id: "ag_bob", tool: "mail.move" }), false);
});

test("«Siempre en esta tarea» is never offered for a delete, a delete tool or a high risk", () => {
  const bob = { seat_id: "ag_bob", tool: "Bash", risk: "low" };
  assert.equal(canAlwaysAllow({ ...bob, payload_redacted: { command: "ls -la" } }), true);
  assert.equal(canAlwaysAllow({ ...bob, payload_redacted: { command: "rm README.md" } }), false);
  assert.equal(canAlwaysAllow({ ...bob, payload_redacted: { command: "ls && rm a.txt" } }), false);
  assert.equal(canAlwaysAllow({ ...bob, risk: "high", payload_redacted: { command: "ls -la" } }), false);
  assert.equal(canAlwaysAllow({ seat_id: "ag_bob", tool: "calendar.delete", risk: "low" }), false);
  assert.equal(canAlwaysAllow({ tool: "Bash", risk: "low" }), false);
});

test("a denial reason is trimmed, cut to 300 characters and empty when there is none", () => {
  assert.equal(REASON_MAX, 300);
  assert.equal(denyReason("  no ahora  "), "no ahora");
  assert.equal(denyReason(null), "");
  assert.equal(denyReason("   "), "");
  assert.equal(denyReason("x".repeat(301)).length, 300);
  assert.equal(denyReason("😀".repeat(301)), "😀".repeat(300));
});

test("the decision's arguments leave out a choice the operator did not make", () => {
  assert.deepEqual(decisionArgs("apr_1", "approve", "c-1", undefined), {
    approvalId: "apr_1",
    decision: "approve",
    clientId: "c-1",
    exclude: undefined,
  });
  assert.equal(decisionArgs("apr_1", "approve", "c-1", [], { wholeTask: true }).wholeTask, true);
  assert.equal(decisionArgs("apr_1", "deny", "c-1", [], { reason: "no" }).reason, "no");
  assert.equal("wholeTask" in decisionArgs("apr_1", "deny", "c-1", []), false);
});

test("a retry reuses its client id only for the same decision and choice", () => {
  const sent = { decision: "approve", exclude: [], wholeTask: true, reason: "" };
  assert.equal(sameDecision(sent, { ...sent }), true);
  assert.equal(sameDecision(sent, { ...sent, wholeTask: false }), false);
  assert.equal(sameDecision(sent, { ...sent, decision: "deny" }), false);
  assert.equal(sameDecision(sent, { ...sent, exclude: ["m1"] }), false);
  assert.equal(sameDecision({ decision: "deny", reason: "no" }, { decision: "deny", reason: "si" }), false);
  assert.equal(sameDecision({ decision: "deny", reason: "no" }, { decision: "deny", reason: "no" }), true);
});

test("the settled line says the scope the host sent", () => {
  assert.equal(resultText("approved", { scope: "task" }), "Permitida para toda esta tarea.");
  assert.equal(resultText("consumed", { scope: "once" }), "Permitida una vez. Solo esta acción.");
  assert.equal(resultText("denied", { reason: "escalated" }), "Denegada. El agente no la ejecutó.");
  assert.equal(resultText("denied", {}), "Denegada. El agente no la ejecutó.");
});

test("the badge of an approval for the whole task says so", () => {
  assert.equal(statusMeta("approved", { scope: "task" }).label, "Permitida para la tarea");
  assert.equal(statusMeta("approved", { scope: "once" }).label, "Permitida una vez");
});

test("a policy decision is not a line of the operator's thread, whatever the row says", () => {
  assert.equal(isPolicyLine({ summary: "Política aprobó: command Bash" }), true);
  assert.equal(isPolicyLine({ summary: "politica aprobó: command Bash" }), true);
  assert.equal(isPolicyLine({ level: "policy", summary: "Bash" }), true);
  assert.equal(isPolicyLine({ summary: "Ernesto terminó" }), false);
  assert.equal(isPolicyLine({ level: "lead", summary: "Responsable aprobó" }), false);
});

test("the plain card shows only where the action lands; the rest is technical", () => {
  const entry = {
    paths: ["C:/p/a.txt"],
    cwd: "C:/p",
    risk: "high",
    escalated_from: "lead",
    action_digest: "sha256:bcca0172345678901234",
  };
  const { shown, technical } = detailFacts(entry);
  assert.deepEqual(shown, [["Ruta", "C:/p/a.txt"]]);
  assert.deepEqual(technical.map(([label]) => label), ["Carpeta", "Riesgo", "Nivel", "Código"]);
  assert.deepEqual(detailFacts({ cwd: "" }).shown, []);
});

test("the strip counts the open conversation's requests and those with none, never another's", () => {
  assert.equal(stripBelongs({ conversation_id: "cnv_1" }, "cnv_1"), true);
  assert.equal(stripBelongs({ conversation_id: "cnv_2" }, "cnv_1"), false);
  assert.equal(stripBelongs({}, "cnv_1"), true);
  assert.equal(stripBelongs({ conversation_id: "cnv_1" }, null), false);
});

test("a request waits only while pending and not answered here", () => {
  assert.equal(isPendingHere({ id: "apr_strip_1", status: "pending" }), true);
  assert.equal(isPendingHere({ id: "apr_strip_2", status: "denied" }), false);
  patchLocal("apr_strip_3", { decided: "approve" });
  assert.equal(isPendingHere({ id: "apr_strip_3", status: "pending" }), false);
});

test("a shell request uses the agent's headline, except a delete, which is named from the command", () => {
  const shell = { tool: "Bash", agent_label: "Ernesto", headline: "Crear redi-uno.txt con el texto uno", summary: "printf 'uno' > redi-uno.txt" };
  assert.match(humanHeadline(shell), /quiere crear redi-uno\.txt con el texto uno$/);
  const lie = { ...shell, summary: "rm -rf README.md", paths: ["C:/x/README.md"] };
  assert.match(humanHeadline(lie), /quiere borrar «README\.md»$/);
});

test("an English headline from the agent never reaches the card or its line: its verb and file name the action", () => {
  const write = { tool: "Bash", agent_label: "Ernesto", headline: 'create redi-ocho.txt with text "uno" and verify', summary: "" };
  assert.equal(humanHeadline(write), "Ernesto quiere guardar «redi-ocho.txt»");
  assert.equal(settledLine("approved", { ...write, scope: "once" }), "✓ Permitido · guardar «redi-ocho.txt»");
  assert.equal(detailFacts(write).technical[0][1], 'create redi-ocho.txt with text "uno" and verify');
  const run = { tool: "Bash", agent_label: "Ernesto", headline: "run the test suite", summary: "" };
  assert.equal(humanHeadline(run), "Ernesto quiere ejecutar un comando");
  assert.equal(humanHeadline({ ...run, paths: ["C:/p/build.log"] }), "Ernesto quiere ejecutar «build.log»");
  assert.equal(humanHeadline({ ...run, headline: "delete old notes", paths: ["C:/p/a.txt"] }), "Ernesto quiere borrar «a.txt»");
  assert.equal(humanHeadline({ ...run, headline: "Ejecutar las pruebas" }), "Ernesto quiere ejecutar las pruebas");
  // An English sentence that opens with no known verb is still English.
  const odd = { ...run, headline: "read-only check of workspaces and search for bench-proj" };
  assert.equal(humanHeadline(odd), "Ernesto quiere ejecutar un comando");
  assert.equal(humanHeadline({ ...odd, paths: ["C:/p/a.txt"] }), "Ernesto quiere ejecutar «a.txt»");
});

test("a look keeps its command under «Detalles técnicos», first; a risky or branch-changing line is no look", () => {
  const command = 'pwd && echo "--- LS ---" && ls -la && git branch --show-current && git status --short --branch && git log --oneline -n 5';
  const ernesto = { tool: "Bash", agent_label: "Ernesto", cwd: "C:/p/bench", risk: "low", payload_redacted: { command } };
  assert.equal(humanHeadline(ernesto), "Ernesto quiere mirar el proyecto");
  assert.equal(riskNote(ernesto), "");
  assert.equal(cardCommand(ernesto), "");
  assert.deepEqual(detailFacts(ernesto).technical[0], ["Comando", command]);
  const high = { ...ernesto, risk: "high" };
  assert.equal(humanHeadline(high), "Ernesto quiere ejecutar un comando");
  assert.equal(riskNote(high), "Puede cambiar o borrar archivos.");
  assert.equal(cardCommand(high), command);
  assert.equal(detailFacts(high).technical.some(([label]) => label === "Comando"), false);
});

test("«git branch» is a look only when it lists; a name or a delete flag changes branches", () => {
  for (const command of ["git branch", "git branch -a", "git branch --list", "git branch --show-current"]) {
    assert.equal(isLookCommand(command), true, command);
  }
  for (const command of ["git branch nueva", "git branch -D main", "git branch -m a b"]) {
    assert.equal(isLookCommand(command), false, command);
  }
});

test("«git -C <folder> status» reads like «git status»", () => {
  assert.equal(isLookCommand('ls -la /c/Users/Max/bench-proj && echo "---" && git -C /c/Users/Max/bench-proj status'), true);
  assert.equal(isLookCommand("git -C /c/Users/Max/bench-proj commit -m x"), false);
  assert.equal(isLookCommand("git -C /c/Users/Max/bench-proj push"), false);
});
