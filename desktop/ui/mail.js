// Ajustes → Correo: the accounts, sign-in by device code, «Permitir enviar» per
// account and the way to open a draft by its id. The host decides every answer;
// this screen asks, shows the code, and says what came back in the host's words.

import { $, copyText, invoke } from "./core.js";
import { element } from "./approval-card.js";
import { openDraft } from "./mail-draft.js";
import { kindPicker } from "./mail-kinds.js";
import { errorMessage } from "./format.js";
import {
  accountKindLabel,
  accountsState,
  countdownFor,
  expiryMs,
  secondsUntil,
  signInOutcome,
  signInTenant,
} from "./mail-model.js";

const ONE_SECOND = 1000;
const DEFAULT_POLL_SECONDS = 5;

// The device code on screen, if any: a sign-in or a send consent. Only one runs.
let flow = null;
let accounts = null;
let revoking = null;
let notice = "";
let ui = null;

export function mountMailSettings() {
  const section = element("section", "mail-settings");
  section.id = "mail-settings";
  section.setAttribute("aria-labelledby", "mail-settings-title");
  const title = element("h2", null, "Correo");
  title.id = "mail-settings-title";
  ui = {
    notice: element("p", "hint"),
    accounts: element("div", "mail-accounts"),
    code: element("div", "mail-code"),
    connect: buildConnect(),
    open: buildOpen(),
  };
  ui.notice.setAttribute("role", "status");
  ui.notice.setAttribute("aria-live", "polite");
  section.append(
    title,
    element("p", "hint", "Los agentes preparan borradores. Enviar lo apruebas tú, con 10 segundos para cancelar."),
    ui.notice,
    ui.accounts,
    ui.code,
    ui.connect.form,
    ui.open.form,
  );
  $("settings-mail").append(section);
  section.addEventListener("click", onClick);
  document.addEventListener("settings:open", loadAccounts);
  document.addEventListener("mail:refresh", loadAccounts);
  render();
}

function buildConnect() {
  const form = element("form", "mail-form");
  form.id = "mail-connect";
  form.setAttribute("aria-label", "Conectar cuenta");
  const name = element("input");
  name.id = "mail-account-name";
  name.type = "text";
  name.maxLength = 64;
  name.autocomplete = "off";
  name.spellcheck = false;
  const kinds = kindPicker();
  const submit = element("button", "btn primary btn-block", "Conectar cuenta");
  submit.type = "submit";
  form.append(
    element("label", "field-label", "Nombre para esta cuenta"),
    name,
    element("span", "field-label", "Tipo de cuenta"),
    kinds.group,
    submit,
  );
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const label = name.value.trim();
    if (!label) {
      notice = "Escribe un nombre para la cuenta.";
      render();
      return;
    }
    const tenant = signInTenant(kinds.selected());
    beginFlow("sign-in", label, () => invoke("mail_login_start", { account: label, tenant }), "mail_login_status");
  });
  return { form, name };
}

function buildOpen() {
  const form = element("form", "mail-form");
  form.id = "mail-open";
  form.setAttribute("aria-label", "Abrir borrador por id");
  const select = element("select");
  select.id = "mail-open-account";
  const id = element("input");
  id.id = "mail-open-id";
  id.type = "text";
  id.autocomplete = "off";
  id.spellcheck = false;
  const submit = element("button", "btn secondary btn-block", "Abrir borrador");
  submit.type = "submit";
  form.append(
    element("h3", null, "Abrir borrador por id"),
    element("label", "field-label", "Cuenta"),
    select,
    element("label", "field-label", "Id del borrador"),
    id,
    submit,
  );
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const account = select.value;
    const draftId = id.value.trim();
    if (!account || !draftId) {
      notice = "Elige una cuenta y escribe el id del borrador.";
      render();
      return;
    }
    openDraft(account, draftId);
  });
  return { form, select, id };
}

function actionButton(label, action, account, className) {
  const button = element("button", className, label);
  button.type = "button";
  button.dataset.mailAction = action;
  if (account) button.dataset.account = account;
  return button;
}

// Every click on the section is handled here, from the data the buttons carry.
function onClick(event) {
  const button = event.target.closest("[data-mail-action]");
  if (!button) return;
  const { mailAction: action, account } = button.dataset;
  if (action === "toggle-send") {
    if (button.getAttribute("aria-checked") === "true") {
      revoking = account;
      render();
    } else {
      beginFlow("consent", account, () => invoke("mail_send_enable", { account }), "mail_send_enable_status");
    }
  } else if (action === "confirm-revoke") {
    revoke(account);
  } else if (action === "cancel-revoke") {
    revoking = null;
    render();
  } else if (action === "start-connect") {
    ui.connect.name.focus();
    ui.connect.form.scrollIntoView({ block: "center" });
  } else if (action === "copy-code") {
    copyText(flow.login.user_code, "Código copiado");
  } else if (action === "cancel-code") {
    stopFlow();
    notice = "";
    render();
  }
}

async function loadAccounts() {
  try {
    accounts = await invoke("mail_accounts");
  } catch (error) {
    accounts = [];
    notice = errorMessage(error);
  }
  render();
}

async function revoke(account) {
  revoking = null;
  try {
    await invoke("mail_send_disable", { account });
    notice = `Envío revocado en ${account}.`;
  } catch (error) {
    notice = errorMessage(error);
  }
  await loadAccounts();
}

function stopFlow() {
  if (!flow) return;
  clearTimeout(flow.timer);
  clearInterval(flow.tick);
  flow = null;
}

// Shows the code at once, then polls the host until it answers for good. The
// poll stops when the code runs out, when the host says so, or when closed.
async function beginFlow(kind, account, begin, statusCommand) {
  stopFlow();
  notice = "";
  let login;
  try {
    login = await begin();
  } catch (error) {
    notice = errorMessage(error);
    render();
    return;
  }
  const current = {
    kind,
    account,
    login,
    statusCommand,
    untilMs: expiryMs(login.expires_in),
    timer: null,
    tick: setInterval(redrawCountdown, ONE_SECOND),
  };
  flow = current;
  current.timer = setTimeout(() => poll(current), intervalMs(login));
  render();
}

function intervalMs(login) {
  return Math.max(1, login.interval || DEFAULT_POLL_SECONDS) * ONE_SECOND;
}

async function poll(current) {
  let status;
  try {
    status = await invoke(current.statusCommand, { account: current.account });
  } catch (error) {
    if (flow === current) {
      stopFlow();
      notice = errorMessage(error);
      render();
    }
    return;
  }
  if (flow !== current) return;
  const outcome = signInOutcome(status);
  if (!outcome.done) {
    current.timer = setTimeout(() => poll(current), intervalMs(current.login));
    return;
  }
  stopFlow();
  notice = outcome.ok && current.kind === "consent" ? `Envío permitido en ${current.account}.` : outcome.text;
  await loadAccounts();
}

// Only the number moves each second; a full redraw would drop the focus.
function redrawCountdown() {
  if (!flow) return;
  if (secondsUntil(flow.untilMs) === 0) {
    stopFlow();
    notice = "Este código ha caducado. Vuelve a empezar.";
    render();
    return;
  }
  const node = document.getElementById("device-countdown");
  if (node) node.textContent = countdownFor(flow.untilMs);
}

function render() {
  ui.notice.textContent = notice;
  renderAccounts();
  renderCode();
  renderOpenChoices();
}

function renderAccounts() {
  const state = accountsState(accounts);
  if (state === "loading") {
    ui.accounts.replaceChildren(element("p", "hint", "Cargando cuentas…"));
    return;
  }
  if (state === "empty") {
    ui.accounts.replaceChildren(connectCard());
    return;
  }
  const list = element("ul", "mail-account-list");
  list.append(...accounts.map(accountRow));
  ui.accounts.replaceChildren(list);
}

// With no account yet, the section says why and leads to the sign-in form below.
function connectCard() {
  const card = element("section", "card mail-connect-card");
  card.setAttribute("aria-labelledby", "mail-connect-card-title");
  const title = element("h3", null, "Conecta tu correo");
  title.id = "mail-connect-card-title";
  card.append(
    title,
    element(
      "p",
      "hint",
      "Conecta una cuenta para que tus agentes preparen borradores. Enviar siempre necesita tu aprobación en el chat.",
    ),
    actionButton("Conectar una cuenta", "start-connect", null, "btn primary btn-block"),
  );
  return card;
}

function accountRow(account) {
  const row = element("li", "mail-account");
  const info = element("div", "mail-account-info");
  const who = [accountKindLabel(account.tenant), account.upn].filter(Boolean).join(" · ");
  info.append(element("strong", null, account.label), element("span", "hint", who));
  row.append(info);
  if (revoking === account.label) {
    row.append(revokeConfirm(account.label));
    return row;
  }
  const toggle = element("button", "mail-switch");
  toggle.type = "button";
  toggle.setAttribute("role", "switch");
  toggle.setAttribute("aria-checked", String(Boolean(account.send_enabled)));
  toggle.dataset.mailAction = "toggle-send";
  toggle.dataset.account = account.label;
  toggle.append(element("span", "mail-switch-label", "Permitir enviar"), element("span", "mail-switch-track"));
  row.append(
    toggle,
    element(
      "p",
      "hint",
      account.send_enabled
        ? "Envío permitido: solo tras tu aprobación y con 10 s para cancelar."
        : "Solo borradores: aquí no se envía nada.",
    ),
  );
  return row;
}

function revokeConfirm(label) {
  const box = element("div", "mail-revoke");
  box.setAttribute("role", "group");
  box.setAttribute("aria-label", `Revocar el envío en ${label}`);
  box.append(
    element("p", null, `¿Dejar de permitir el envío en ${label}? Los borradores siguen guardados.`),
    actionButton("Revocar", "confirm-revoke", label, "btn destructive"),
    actionButton("Cancelar", "cancel-revoke", label, "btn ghost"),
  );
  return box;
}

function renderCode() {
  if (!flow) {
    ui.code.replaceChildren();
    return;
  }
  const heading = element(
    "h3",
    null,
    flow.kind === "consent" ? `Confirma el permiso para enviar desde ${flow.account}` : `Inicia sesión en ${flow.account}`,
  );
  const countdown = element("p", "device-countdown", countdownFor(flow.untilMs));
  countdown.id = "device-countdown";
  countdown.setAttribute("role", "timer");
  countdown.setAttribute("aria-label", "Tiempo restante del código");
  const card = element("section", "card device-code");
  card.append(
    heading,
    element(
      "p",
      "hint",
      `Abre ${flow.login.verification_uri} en el navegador e introduce este código. Aquí no se piden contraseñas.`,
    ),
    element("code", "device-code-value", flow.login.user_code),
    countdown,
    actionButton("Copiar código", "copy-code", null, "btn secondary btn-block"),
    actionButton("Cancelar", "cancel-code", null, "btn ghost btn-block"),
  );
  ui.code.replaceChildren(card);
}

// The open-by-id form lists the accounts it can open a draft from.
function renderOpenChoices() {
  const select = ui.open.select;
  const labels = accounts ? accounts.map((account) => account.label) : [];
  select.replaceChildren(...labels.map((label) => element("option", null, label)));
  select.disabled = !labels.length;
  ui.open.id.disabled = !labels.length;
}
