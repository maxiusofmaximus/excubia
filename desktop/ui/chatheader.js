import { t } from "./i18n.js";
import { app, $ } from "./core.js";
import { paintAvatar } from "./avatar.js";
import { formatTime } from "./chatlist.js";
import { chat, connection, hostName, offline } from "./chatstate.js";
import { openStep } from "./chat-progress-model.js";
import { conversationTitle } from "./chat-row-model.js";

export function renderHeader() {
  const alfred =
    (chat.home && chat.home.bootstrap && chat.home.bootstrap.alfred.name) || "Alfred";
  const title = chat.conversation ? conversationTitle(chat.conversation) : alfred;
  $("chat-title").textContent = title;
  const seat = chat.home && chat.home.bootstrap ? chat.home.bootstrap.alfred : null;
  paintAvatar($("chat-avatar"), {
    name: alfred,
    characterId: "alfred",
    status: seat && seat.status,
    look: chat.waiting ? "thinking" : null,
    px: 64,
  });
  const state = connection();
  // The subtitle names the agent; it says more only while something is off or running.
  let subtitle = alfred;
  if (chat.conversation && chat.conversation.archived) subtitle = t("chatheader.subtitle_archived", { name: alfred });
  else if (chat.waiting) subtitle = t("chatheader.subtitle_working", { name: alfred, step: openStep(chat) || t("chatheader.working") });
  else if (state.kind === "offline" || state.kind === "fatal") subtitle = t("chatheader.subtitle_offline", { name: alfred });
  else if (state.kind === "connecting") subtitle = t("chatheader.subtitle_connecting", { name: alfred });
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
      [t("pair.restart_host"), { action: "restart-host" }, "btn secondary btn-small"],
    ]);
    const strong = document.createElement("strong");
    strong.textContent = t("chatheader.host_failed");
    text.append(strong, document.createTextNode(host.error || t("chatheader.host_failed_hint")));
    element.classList.remove("hidden");
    return;
  }
  if (offline()) {
    const { element, text } = banner([
      [t("failure.reconnect"), { action: "reconnect" }, "btn primary btn-small"],
      ...(state.kind === "fatal"
        ? [[t("agents.pair_again"), { action: "settings" }, "btn ghost btn-small"]]
        : []),
    ]);
    element.classList.add(state.kind === "fatal" ? "fatal" : "offline");
    const when = chat.lastSync ? formatTime(new Date(chat.lastSync).toISOString()) : "—";
    const strong = document.createElement("strong");
    strong.textContent = t("chatheader.offline_host", { host: hostName() });
    text.append(
      strong,
      document.createTextNode(t("chatheader.offline_last_sync", { when })),
    );
    element.classList.remove("hidden");
    return;
  }
  if (!chat.home && chat.loadFailure) {
    const { element, text } = banner([
      [t("failure.open_settings"), { action: "settings" }, "btn secondary btn-small"],
      [t("agents.retry"), { action: "reconnect" }, "btn ghost btn-small"],
    ]);
    const strong = document.createElement("strong");
    strong.textContent = t("chatheader.load_failed");
    text.append(strong, document.createTextNode(chat.loadFailure.message || ""));
    element.classList.remove("hidden");
    return;
  }
  if (chat.conversation && chat.conversation.archived) {
    const { element, text } = banner([
      [t("failure.unarchive"), { action: "unarchive" }, "btn secondary btn-small"],
    ]);
    const strong = document.createElement("strong");
    strong.textContent = t("chatheader.archived_banner");
    text.append(
      strong,
      document.createTextNode(t("chatheader.archived_hint")),
    );
    element.classList.remove("hidden");
    return;
  }
  // The cached copy is not a reason for a banner: the offline banner above says when the PC is gone.
  $("chat-banner").classList.add("hidden");
}
