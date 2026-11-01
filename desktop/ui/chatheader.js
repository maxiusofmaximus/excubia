import { app, $ } from "./core.js";
import { paintAvatar } from "./avatar.js";
import { formatTime } from "./chatlist.js";
import { chat, connection, hostName, offline } from "./chatstate.js";
import { openStep } from "./chat-progress-model.js";

export function renderHeader() {
  const alfred =
    (chat.home && chat.home.bootstrap && chat.home.bootstrap.alfred.name) || "Alfred";
  const title = chat.conversation ? chat.conversation.title : alfred;
  $("chat-title").textContent = title;
  const seat = chat.home && chat.home.bootstrap ? chat.home.bootstrap.alfred : null;
  paintAvatar($("chat-avatar"), {
    name: alfred,
    characterId: "alfred",
    status: chat.waiting ? "working" : seat && seat.status,
    px: 34,
  });
  $("btn-chat-more").disabled = !chat.conversation;
  const state = connection();
  let subtitle = `${alfred} · CTO`;
  if (chat.conversation && chat.conversation.archived) subtitle = `${alfred} · archivada`;
  else if (chat.waiting) subtitle = `${alfred} · ${openStep(chat) || "trabajando"}`;
  else if (state.kind === "connected") subtitle = `${alfred} · en vivo`;
  else if (state.kind === "offline" || state.kind === "fatal") subtitle = `${alfred} · sin conexión`;
  else if (state.kind === "connecting") subtitle = `${alfred} · conectando…`;
  $("chat-subtitle").textContent = subtitle;
}

function banner(actions) {
  const element = $("chat-banner");
  element.replaceChildren();
  element.className = "chat-banner";
  const icon = document.createElement("span");
  icon.className = "banner-icon";
  icon.setAttribute("aria-hidden", "true");
  icon.textContent = "!";
  const text = document.createElement("span");
  text.dataset.bannerText = "";
  element.append(icon, text);
  if (actions) {
    const group = document.createElement("span");
    group.className = "banner-actions";
    for (const [label, data, style] of actions) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = style || "btn ghost btn-small";
      button.textContent = label;
      for (const [name, value] of Object.entries(data)) button.dataset[name] = value;
      group.append(button);
    }
    element.append(group);
  }
  return { element, text };
}

export function renderBanner() {
  const host = app.boot && app.boot.host;
  const state = connection();
  if (host && host.state === "failed") {
    const { element, text } = banner([
      ["Reiniciar el host", { action: "restart-host" }, "btn secondary btn-small"],
    ]);
    const strong = document.createElement("strong");
    strong.textContent = "El host local no arrancó. ";
    text.append(strong, document.createTextNode(host.error || "Revisa el log y vuelve a intentarlo."));
    element.classList.remove("hidden");
    return;
  }
  if (offline()) {
    const { element, text } = banner([
      ["Reconectar", { action: "reconnect" }, "btn primary btn-small"],
      ...(state.kind === "fatal"
        ? [["Volver a emparejar", { action: "settings" }, "btn ghost btn-small"]]
        : []),
    ]);
    element.classList.add(state.kind === "fatal" ? "fatal" : "offline");
    const when = chat.lastSync ? formatTime(new Date(chat.lastSync).toISOString()) : "—";
    const strong = document.createElement("strong");
    strong.textContent = `Sin conexión con ${hostName()}. `;
    text.append(
      strong,
      document.createTextNode(
        `Última sincronización ${when}. Lo que ya está cargado sigue legible; no se puede enviar.`,
      ),
    );
    element.classList.remove("hidden");
    return;
  }
  if (!chat.home && chat.loadFailure) {
    const { element, text } = banner([
      ["Abrir ajustes", { action: "settings" }, "btn secondary btn-small"],
      ["Reintentar", { action: "reconnect" }, "btn ghost btn-small"],
    ]);
    const strong = document.createElement("strong");
    strong.textContent = "No se pudo cargar el chat. ";
    text.append(strong, document.createTextNode(chat.loadFailure.message || ""));
    element.classList.remove("hidden");
    return;
  }
  if (chat.conversation && chat.conversation.archived) {
    const { element, text } = banner([
      ["Desarchivar", { action: "unarchive" }, "btn secondary btn-small"],
    ]);
    const strong = document.createElement("strong");
    strong.textContent = "Conversación archivada. ";
    text.append(
      strong,
      document.createTextNode(
        "Puedes leerla, fijarla o moverla; para escribir, desarchívala.",
      ),
    );
    element.classList.remove("hidden");
    return;
  }
  if (chat.cacheOnly) {
    const { element, text } = banner([]);
    text.textContent = "Mostrando la última copia legible; aún no hay conexión con el host.";
    element.classList.remove("hidden");
    return;
  }
  $("chat-banner").classList.add("hidden");
}
