// The GitHub link section of the «Enlazar GitHub» approval card (GITHUB-INTEGRACION): progress, the
// one-time code, the fixed GitHub button, cancel and the result, all on the same card. The state comes
// from the host's signed read. The code lives only in this element's text: nothing stores it, and it is
// gone when the card leaves the page.
import { copyText, invoke } from "./core.js";
import { t } from "./i18n.js";
import { GITHUB_DEVICE_URL, githubFacts, githubPolls, githubStage } from "./github-link-model.js";

const POLL_MS = 1000;
// A started question with no link after this many polls (about 20 s) is a failure, not a wait.
const STALE_POLLS = 20;

let timer = null;

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined && text !== null) node.textContent = text;
  return node;
}

// The section for one approval. Its stage comes from the approval's status until the link's state arrives.
export function githubSection(entry, status) {
  const section = el("section", "github-link");
  section.dataset.githubLink = entry.id;
  section.dataset.approvalStatus = status;
  section.dataset.stale = "0";
  paintStage(section, githubStage(status, null, entry.id), null);
  ensurePolling();
  return section;
}

function ensurePolling() {
  if (timer === null && typeof setInterval === "function") timer = setInterval(refresh, POLL_MS);
}

// One read of the link's state, shared by every open section of this profile.
async function refresh() {
  const sections = [...document.querySelectorAll("[data-github-link]")].filter(
    (section) => section.isConnected && githubPolls(section.dataset.stage || "awaiting"),
  );
  if (!sections.length) return;
  let card = null;
  try {
    card = await invoke("chat_github_link_state");
  } catch {
    return;
  }
  for (const section of sections) {
    if (!section.isConnected) continue;
    let stage = githubStage(section.dataset.approvalStatus, card, section.dataset.githubLink);
    if (stage === "starting") {
      section.dataset.stale = String(Number(section.dataset.stale || 0) + 1);
      if (Number(section.dataset.stale) > STALE_POLLS) stage = "failed";
    } else {
      section.dataset.stale = "0";
    }
    paintStage(section, stage, card);
  }
}

function paintStage(section, stage, card) {
  const showingCode = section.querySelector(".github-code") !== null;
  // Redraw only when the stage moves or the code first appears: the buttons keep their focus.
  if (section.dataset.stage === stage && (stage !== "code_ready" || showingCode)) return;
  const facts = githubFacts(stage, card);
  section.dataset.stage = stage;
  section.replaceChildren(el("p", "github-stage", t(`github.stage.${stage}`)));

  if (facts.code) {
    section.append(
      el("p", "github-code-label", t("github.code_label")),
      el("code", "github-code", facts.code),
      el("p", "github-hint", t("github.hint")),
    );
    section.append(button(t("github.copy_code"), "btn secondary", () => copyText(facts.code, t("github.code_copied"))));
  }
  if (facts.showOpen) {
    // The one fixed GitHub page: the host's answer never names an address.
    section.append(button(t("github.open"), "btn secondary", () => openFixed(GITHUB_DEVICE_URL)));
  }
  if (facts.showCancel) {
    section.append(button(t("github.cancel"), "btn ghost", () => cancelLink(section.dataset.githubLink)));
  }
  if (facts.finished && facts.resultKey) {
    const key = stage === "linked" && !facts.login ? "github.result.linked_plain" : facts.resultKey;
    section.append(el("p", "github-result", t(key, facts.login ? { login: facts.login } : undefined)));
  }
}

function button(label, className, onClick) {
  const node = el("button", `${className} github-btn`, label);
  node.type = "button";
  node.addEventListener("click", onClick);
  return node;
}

function openFixed(url) {
  invoke("open_link", { url }).catch(() => {});
}

// Cancels this question's link. The next read shows the cancelled stage.
async function cancelLink(approvalId) {
  try {
    await invoke("chat_github_link_cancel", { approvalId });
  } catch {
    // The next read shows the state; a failed cancel leaves the link running and visible.
  }
  refresh();
}
