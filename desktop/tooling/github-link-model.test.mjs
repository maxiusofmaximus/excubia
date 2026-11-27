// The «Enlazar GitHub» card rules (GITHUB-INTEGRACION): a card speaks only for its own question, the
// code shows only at code_ready of that question, the button is the one fixed page, and both languages
// carry the same keys. Run with: node --test tooling/github-link-model.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { dictionaries, setLang, t } from "../ui/i18n.js";
import {
  GITHUB_DEVICE_URL,
  githubCause,
  githubExplanation,
  githubFacts,
  githubMissing,
  githubNewer,
  githubPolls,
  githubStage,
  isGithubLinkTool,
} from "../ui/github-link-model.js";

const served = (phase, extra = {}) => ({ phase, approval_ids: ["apr_mine"], ...extra });

test("the button always opens the one fixed GitHub page", () => {
  assert.equal(GITHUB_DEVICE_URL, "https://github.com/login/device");
  const facts = githubFacts("code_ready", served("code_ready", { code: "ABCD-1234", url: GITHUB_DEVICE_URL }));
  assert.equal(facts.openUrl, "https://github.com/login/device");
});

test("the status of the approval comes first; a denied or expired question ends at once", () => {
  assert.equal(githubStage("pending", null, "apr_mine"), "awaiting");
  assert.equal(githubStage("denied", served("linked"), "apr_mine"), "declined");
  assert.equal(githubStage("expired", null, "apr_mine"), "expired");
  assert.equal(githubStage("approved", null, "apr_mine"), "starting");
  assert.equal(githubStage("consumed", null, "apr_mine"), "starting");
});

test("a card serves only the question it names: another question's code is never shown", () => {
  const foreign = served("code_ready", { code: "WXYZ-9999", approval_ids: ["apr_other"] });
  const stage = githubStage("approved", foreign, "apr_mine");
  assert.equal(stage, "superseded");
  const facts = githubFacts(stage, foreign);
  assert.equal(facts.code, null, "a superseded card carries no code");
  assert.equal(facts.showOpen, false);
  assert.equal(facts.finished, true);
});

test("a question just approved is starting while the card still shows an earlier attempt", () => {
  // The poll can read the previous attempt's terminal card before this question's attempt begins.
  const earlier = { phase: "linked", login: "octocat", approval_ids: ["apr_01"] };
  const stage = githubStage("approved", earlier, "apr_02");
  assert.equal(stage, "starting", "an earlier attempt does not close a new question");
  assert.equal(githubFacts(stage, earlier).finished, false);
  assert.equal(githubFacts(stage, earlier).resultKey, null);
  const earlierCode = { phase: "code_ready", code: "OLD1-2345", approval_ids: ["apr_01"] };
  assert.equal(githubFacts(githubStage("approved", earlierCode, "apr_02"), earlierCode).code, null, "an earlier code is never shown");
  // Once a later attempt replaced this question, its card is superseded as before.
  assert.equal(githubStage("approved", { phase: "linked", approval_ids: ["apr_03"] }, "apr_02"), "superseded");
  assert.equal(githubStage("consumed", { phase: "linked", approval_ids: ["apr_01"] }, "apr_02"), "starting");
});

test("the code and the open button appear only at code_ready, and only for the served question", () => {
  const card = served("code_ready", { code: "ABCD-1234" });
  const facts = githubFacts(githubStage("approved", card, "apr_mine"), card);
  assert.equal(facts.code, "ABCD-1234");
  assert.equal(facts.showOpen, true);
  assert.equal(facts.showCancel, true);
  const waiting = githubFacts("waiting_code", served("waiting_code"));
  assert.equal(waiting.code, null);
  assert.equal(waiting.showOpen, true, "the page is open while the code is requested");
});

test("cancel is offered only while this question's link runs; a finished card shows its result", () => {
  assert.equal(githubFacts("linked", served("linked", { login: "octocat" })).showCancel, false);
  const linked = githubFacts("linked", served("linked", { login: "octocat" }));
  assert.equal(linked.finished, true);
  assert.equal(linked.resultKey, "github.result.linked");
  assert.equal(linked.login, "octocat");
  assert.equal(githubFacts("failed", served("failed")).resultKey, "github.result.failed");
  assert.equal(githubFacts("expired", served("expired")).resultKey, "github.result.expired");
  assert.equal(githubFacts("declined", null).resultKey, "github.result.declined");
});

test("only the approval of github.link takes this card, and only pending or started questions poll", () => {
  assert.equal(isGithubLinkTool({ tool: "github.link" }), true);
  assert.equal(isGithubLinkTool({ tool: "mail.send" }), false);
  assert.equal(isGithubLinkTool(null), false);
  assert.equal(githubPolls("awaiting"), false, "a question that waits for the operator is not polled");
  assert.equal(githubPolls("starting"), true);
  assert.equal(githubPolls("code_ready"), true);
  assert.equal(githubPolls("linked"), false);
  assert.equal(githubPolls("superseded"), false);
});

test("a failed link names its cause from the host's code, and every cause has words in both languages", () => {
  const failed = (failure, phase = "failed") => ({ phase, failure, approval_ids: ["apr_mine"] });
  assert.equal(githubCause("failed", failed("download_failed")), "offline");
  assert.equal(githubCause("failed", failed("login_failed")), "login");
  assert.equal(githubCause("failed", failed("digest_mismatch")), "install");
  assert.equal(githubCause("failed", failed("spawn_failed")), "other");
  assert.equal(githubCause("failed", failed(undefined)), "other", "a failure without a code is other");
  assert.equal(githubCause("setup_failed", failed("identity_failed", "setup_failed")), "other");
  assert.equal(githubCause("linked", served("linked")), null);
  assert.equal(githubCause("expired", served("expired")), null, "an expired request states its cause in its result");
  assert.equal(githubFacts("failed", failed("login_failed")).cause, "login");
  for (const lang of ["es", "en"]) {
    setLang(lang);
    for (const cause of ["offline", "install", "login", "other"]) {
      const text = t(`github.cause.${cause}`);
      assert.ok(text && !text.startsWith("github."), `${lang} has the ${cause} cause`);
      assert.ok(!/_failed|download|digest|gh |setup/.test(text), `${lang} ${cause} names no internal term`);
    }
  }
  setLang("es");
});

test("retry is offered only where the link failed or the request expired, never over another question's card", () => {
  for (const stage of ["failed", "setup_failed", "expired"]) {
    assert.equal(githubFacts(stage, served(stage)).showRetry, true, `${stage} offers retry`);
  }
  for (const stage of ["linked", "code_ready", "waiting_code", "cancelled", "declined", "superseded", "starting"]) {
    assert.equal(githubFacts(stage, served(stage)).showRetry, false, `${stage} offers no retry`);
  }
});

test("the real sequence: a retry's card shows its own code, and the earlier card keeps its failure without retry", () => {
  // Attempt 1 (apr_01) reaches code_ready, then fails with login_failed.
  const first = (phase, extra = {}) => ({ phase, approval_ids: ["apr_01"], ...extra });
  assert.equal(githubFacts(githubStage("consumed", first("code_ready", { code: "AAAA-1111" }), "apr_01"), first("code_ready", { code: "AAAA-1111" })).code, "AAAA-1111");
  const failed = first("failed", { failure: "login_failed" });
  const oldFacts = githubFacts(githubStage("consumed", failed, "apr_01"), failed);
  assert.equal(oldFacts.stage, "failed");
  assert.equal(oldFacts.cause, "login");
  assert.equal(oldFacts.showRetry, true, "a failed card offers retry while nothing newer exists");

  // «Volver a intentar» opens apr_02, still pending: the old card is history with no retry.
  const ids = ["apr_01", "apr_02"];
  assert.equal(githubNewer("apr_01", ids), true);
  assert.equal(githubNewer("apr_02", ids), false);
  assert.equal(githubFacts(oldFacts.stage, failed, githubNewer("apr_01", ids)).showRetry, false, "old card: no retry");
  assert.equal(githubStage("pending", failed, "apr_02"), "awaiting");

  // The operator allows apr_02: the host still shows attempt 1, so the new card is starting, not failed,
  // and shows no earlier failure or code.
  const starting = githubStage("approved", failed, "apr_02");
  assert.equal(starting, "starting");
  const startingFacts = githubFacts(starting, null, false);
  assert.equal(startingFacts.code, null);
  assert.equal(startingFacts.cause, null);
  assert.equal(startingFacts.showRetry, false);

  // The host starts attempt 2: its card serves apr_02 and shows its own code; the old card is still history.
  const second = { phase: "code_ready", code: "BBBB-2222", approval_ids: ["apr_02"] };
  const newStage = githubStage("consumed", second, "apr_02");
  assert.equal(newStage, "code_ready");
  const newFacts = githubFacts(newStage, second, githubNewer("apr_02", ids));
  assert.equal(newFacts.code, "BBBB-2222", "the new card shows its own code");
  assert.equal(newFacts.showOpen, true);
  assert.equal(newFacts.showRetry, false);
  assert.equal(githubStage("consumed", second, "apr_01"), "superseded", "attempt 1 is no longer served");
  assert.equal(githubFacts("superseded", second).code, null, "the old card never shows attempt 2's code");
});

test("a card with no later question is unchanged and a later question is the larger id", () => {
  assert.equal(githubNewer("apr_02", ["apr_01", "apr_02"]), false);
  assert.equal(githubNewer("apr_01", ["apr_01"]), false);
  assert.equal(githubFacts("failed", served("failed"), false).showRetry, true);
  assert.equal(githubFacts("failed", served("failed"), true).showRetry, false);
});

test("ES and EN read the same states with their own words", () => {
  setLang("es");
  try {
    assert.equal(t("github.result.linked", { login: "octocat" }), "✓ Enlazado como octocat. Excubia guarda solo el nombre y el usuario públicos.");
    assert.equal(t("github.allow"), "Permitir");
    assert.equal(t("github.later"), "Ahora no");
    assert.equal(t("github.result.expired"), "El código caducó. Vuelve a pedirlo si lo necesitas.");
  } finally {
    setLang("en");
  }
  assert.equal(t("github.result.linked", { login: "octocat" }), "✓ Linked as octocat. Excubia keeps only the public name and username.");
  assert.equal(t("github.later"), "Not now");
  assert.equal(t("github.result.expired"), "The code expired. Ask again if you still need it.");
});

test("every GitHub key exists in both languages", () => {
  const keys = (dictionary) => Object.keys(dictionary).filter((key) => key.startsWith("github.")).sort();
  assert.deepEqual(keys(dictionaries.es), keys(dictionaries.en));
  assert.ok(keys(dictionaries.es).length > 0);
});

test("the question and the button are fixed words of the desktop in both languages", () => {
  setLang("es");
  assert.equal(t("github.question"), "¿Enlazar GitHub con Excubia?");
  assert.equal(t("github.allow"), "Permitir");
  setLang("en");
  assert.equal(t("github.question"), "Link GitHub to Excubia?");
  assert.equal(t("github.allow"), "Allow");
  setLang("es");
});

test("the explanation is built here from fixed tool names, never from the host text", () => {
  const entry = (missing) => ({
    tool: "github.link",
    summary: "texto del host",
    payload_redacted: { host: "github.com", missing_tools: missing },
  });
  assert.deepEqual(githubMissing(entry(["gh", "rm -rf", "git", "gh"])), ["git", "gh"]);
  assert.deepEqual(githubMissing({ tool: "github.link" }), []);
  setLang("es");
  const one = githubExplanation(["git"], t);
  assert.ok(one.startsWith("Antes instala Git verificado"));
  assert.ok(one.endsWith("nunca la contraseña ni el token."));
  assert.ok(!one.includes("texto del host"));
  assert.equal(githubExplanation([], t), t("github.explain.open"));
  setLang("en");
  assert.ok(githubExplanation(["git", "gh"], t).startsWith("First it installs Git and GitHub CLI, each verified"));
  assert.ok(githubExplanation([], t).startsWith("A one-time code opens on GitHub"));
  setLang("es");
});
