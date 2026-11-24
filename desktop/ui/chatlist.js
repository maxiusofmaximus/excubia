import { locale, t } from "./i18n.js";
import { conversationTitle, isBlankChat, rowBadge } from "./chat-row-model.js";
import { $ } from "./core.js";
import { setNavBadge } from "./nav.js";
import { emptyState } from "./empty-state.js";
import { paintAvatar } from "./avatar.js";
import { EMPTY_TEXT, FILTERS } from "./chat-filter-model.js";
import { progressText } from "./chat-progress-model.js";
import { prune } from "./chat-select-model.js";

const dayNames = () => [
  t("chat.day.sun"), t("chat.day.mon"), t("chat.day.tue"), t("chat.day.wed"), t("chat.day.thu"), t("chat.day.fri"), t("chat.day.sat"),
];
const monthNames = () => [
  t("chat.month.jan"), t("chat.month.feb"), t("chat.month.mar"), t("chat.month.apr"), t("chat.month.may"), t("chat.month.jun"),
  t("chat.month.jul"), t("chat.month.aug"), t("chat.month.sep"), t("chat.month.oct"), t("chat.month.nov"), t("chat.month.dec"),
];

export function formatTime(iso) {
  const date = iso ? new Date(iso) : null;
  if (!date || Number.isNaN(date.valueOf())) return "";
  return date.toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });
}

export function formatWhen(iso) {
  const date = iso ? new Date(iso) : null;
  if (!date || Number.isNaN(date.valueOf())) return "";
  const now = new Date();
  const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate()).valueOf();
  const value = date.valueOf();
  const dayMs = 86_400_000;
  if (value >= midnight) return formatTime(iso);
  if (value >= midnight - dayMs) return t("chat.when.yesterday");
  if (value >= midnight - 6 * dayMs) return dayNames()[date.getDay()];
  if (date.getFullYear() === now.getFullYear()) {
    return `${date.getDate()} ${monthNames()[date.getMonth()]}`;
  }
  return `${date.getDate()} ${monthNames()[date.getMonth()]} ${date.getFullYear()}`;
}

// The open conversation (chat.selectedId) is marked only while the Chat screen is on show;
// the choice itself is kept, so the mark comes back when Chat does.
let openId = null;

function markOpenRow() {
  const shown = !$("view-chat").classList.contains("hidden");
  for (const element of $("chat-groups").querySelectorAll(".chat-row[data-id]")) {
    element.classList.toggle("is-active", shown && element.dataset.id === openId);
  }
}

// showView only changes the screen's class, so the mark follows that class.
new MutationObserver(markOpenRow).observe($("view-chat"), { attributes: true, attributeFilter: ["class"] });

// `view` is { selectedId, progress }: the open conversation and the steps of the running ones.
function row(conversation, view) {
  const element = document.createElement("div");
  element.className = "chat-row";
  element.dataset.id = conversation.id;
  if (conversation.unread > 0) element.classList.add("is-unread");
  const name = conversationTitle(conversation);

  // The option of the history listbox; paintSelection sets aria-selected on each render.
  const main = document.createElement("button");
  main.type = "button";
  main.className = "chat-row-main";
  main.setAttribute("role", "option");
  main.setAttribute("aria-selected", "false");
  main.dataset.select = conversation.id;
  main.setAttribute(
    "aria-label",
    `${name}${conversation.unread ? t("chat.row.unread_aria", { n: conversation.unread }) : ""}`,
  );

  const title = document.createElement("span");
  title.className = "chat-row-title";
  title.textContent = `${conversation.pinned ? "★ " : ""}${name}`;
  title.title = name;

  const meta = document.createElement("span");
  meta.className = "chat-row-meta";
  // Every row is with Alfred, so the meta line says only the time and the state.
  const when = formatWhen(conversation.last_message_at);
  const parts = [];
  if (conversation.archived) parts.push(t("chat.row.archived"));
  if (when) parts.push(when);
  if (conversation.unread) parts.push(t("chat.row.unread", { n: conversation.unread }));
  const step = progressText(view.progress, conversation.id);
  meta.textContent = step || parts.join(" · ") || t("chat.row.no_messages");
  const badge = rowBadge(conversation);
  if (badge) {
    const chip = document.createElement("span");
    chip.className = `chat-row-badge is-${badge.kind}`;
    chip.textContent = badge.text;
    // In the meta line, never inside the title: a long title is cut with an ellipsis.
    meta.prepend(chip);
  }
  main.append(title, meta);

  if (conversation.unread) {
    const dot = document.createElement("span");
    dot.className = "unread-dot";
    dot.setAttribute("aria-hidden", "true");
    element.append(dot);
  }

  const more = document.createElement("button");
  more.type = "button";
  more.className = "row-more";
  more.dataset.menu = conversation.id;
  more.setAttribute("aria-label", t("chat.row.actions_aria", { title: name }));
  more.setAttribute("aria-haspopup", "menu");
  more.textContent = "⋯";

  element.append(main, more);
  return element;
}

function caption(title) {
  const element = document.createElement("section");
  element.className = "group group-caption";
  const head = document.createElement("div");
  head.className = "group-head";
  head.textContent = title;
  element.append(head);
  return element;
}

// A null title draws the rows with no heading (a list with one group needs none).
function group(title, conversations, view, options = {}) {
  const element = document.createElement("section");
  element.className = "group";
  if (title !== null) {
    const head = document.createElement("div");
    head.className = "group-head";
    const label = document.createElement("span");
    label.textContent = title;
    head.append(label);
    if (options.action) {
      const action = document.createElement("button");
      action.type = "button";
      action.className = "group-action";
      action.dataset.action = options.action;
      action.textContent = options.actionLabel || t("chat.group.view");
      head.append(action);
    }
    element.append(head);
  }
  for (const conversation of conversations) {
    element.append(row(conversation, view));
  }
  return element;
}

// The ids of the rows on screen, top to bottom: the order a Shift+click ranges over.
export function visibleOrder() {
  return [...$("chat-groups").querySelectorAll(".chat-row[data-id]")].map((element) => element.dataset.id);
}

// The bar above the list: shown from two picked rows, and while a bulk archive runs.
function renderSelectBar(chat) {
  const progress = chat.bulkProgress;
  const count = chat.selection.ids.size;
  $("chat-select-bar").classList.toggle("hidden", !progress && count < 2);
  $("btn-select-delete").classList.toggle("hidden", Boolean(progress));
  $("btn-select-cancel").classList.toggle("hidden", Boolean(progress));
  $("chat-select-count").textContent = progress
    ? t(progress.archived ? "chat.bulk.archiving" : "chat.bulk.unarchiving", { done: progress.done, total: progress.total })
    : t("chat.bulk.selected", { n: count });
}

// Runs after each history render: drops the rows that left, marks the picked ones.
export function paintSelection(chat) {
  const rows = [...$("chat-groups").querySelectorAll(".chat-row[data-id]")];
  prune(chat.selection, rows.map((element) => element.dataset.id));
  openId = chat.conversation ? chat.conversation.id : null;
  markOpenRow();
  for (const element of rows) {
    const picked = chat.selection.ids.has(element.dataset.id);
    element.classList.toggle("is-selected", picked);
    element.querySelector(".chat-row-main").setAttribute("aria-selected", String(picked));
  }
  renderSelectBar(chat);
}

export function renderHistory(chat) {
  const groups = $("chat-groups");
  groups.replaceChildren();
  const home = chat.home;
  const view = { selectedId: chat.conversation ? chat.conversation.id : null, progress: chat.progress };

  if (!home) {
    for (let index = 0; index < 4; index += 1) {
      const skeleton = document.createElement("div");
      skeleton.className = "skeleton-chat";
      groups.append(skeleton);
    }
    return;
  }

  const selectedId = chat.conversation ? chat.conversation.id : null;
  const conversations = (home.conversations || []).filter((conversation) => !isBlankChat(conversation, selectedId));
  const archived = home.archived || [];

  if (chat.searchResults) {
    if (!chat.searchResults.length) {
      const empty = document.createElement("p");
      empty.className = "history-empty";
      empty.textContent = t("chat.search.none", { query: chat.query });
      groups.append(empty);
    } else {
      groups.append(
        group(t("chat.search.results", { n: chat.searchResults.length }), chat.searchResults, view),
      );
    }
    return;
  }

  if (chat.showArchived) {
    if (!archived.length) {
      const empty = document.createElement("p");
      empty.className = "history-empty";
      empty.textContent = t("chat.archived.none");
      groups.append(empty);
    } else {
      groups.append(group(t("chat.archived.title"), archived, view));
    }
    return;
  }

  if (chat.filter !== "all") {
    const rows = chat.filtered || [];
    if (!rows.length) {
      const empty = document.createElement("p");
      empty.className = "history-empty";
      empty.textContent = EMPTY_TEXT[chat.filter];
      groups.append(empty);
    } else {
      groups.append(group(FILTERS.find((filter) => filter.id === chat.filter).label, rows, view));
    }
    return;
  }

  if (!conversations.length) {
    const onNew = () => $("btn-new-chat").click();
    groups.append(
      emptyState({ icon: "chat", text: t("chat.empty.none"), action: t("chat.empty.new") }, onNew),
    );
    return;
  }

  const pinned = conversations.filter((conversation) => conversation.pinned);
  const sections = home.sections || [];
  const sectionIds = new Set(sections.map((section) => section.id));
  const inSections = conversations.filter(
    (conversation) => !conversation.pinned && conversation.section_id && sectionIds.has(conversation.section_id),
  );
  const recent = conversations.filter(
    (conversation) => !conversation.pinned && !inSections.includes(conversation),
  );

  // «Recientes» names the rest only when pinned rows or sections sit above it.
  const labelled = pinned.length > 0 || sections.length > 0;
  if (pinned.length) groups.append(group(t("chat.group.pinned"), pinned, view));
  if (recent.length) groups.append(group(labelled ? t("chat.group.recent") : null, recent, view));

  if (sections.length) {
    groups.append(caption(t("chat.group.sections")));
    for (const section of sections) {
      const rows = inSections.filter((conversation) => conversation.section_id === section.id);
      groups.append(group(section.name, rows, view));
    }
  }
}

const SEAT_STATUS = {
  draft: t("chat.seat.ready"),
  idle: t("chat.seat.ready"),
  ready: t("chat.seat.ready"),
  working: t("chat.seat.working"),
  running: t("chat.seat.working"),
  blocked: t("chat.seat.blocked"),
  done: t("chat.seat.done"),
};

function splitSeat(title) {
  const match = String(title || "").match(/^(.*?)\s+[—-]\s+(.*)$/);
  return match ? [match[1], match[2]] : null;
}

function seatLabel(agent) {
  const title = (agent.title || "").trim();
  const split = splitSeat(title);
  if (split) return split[0];
  if (title && !/[\\/]/.test(title) && title.length <= 40) return title;
  return agent.agent || t("chat.seat.default");
}

function seatRole(agent) {
  const split = splitSeat((agent.title || "").trim());
  if (split) return split[1];
  return agent.agent || t("chat.seat.default_role");
}

function matchesProject(agent, project) {
  const projectName = (project.name || "").trim().toLowerCase();
  const root = (project.root_label || "").trim().toLowerCase();
  const stateProject = (agent.project || "").trim().toLowerCase();
  if (stateProject && (stateProject === projectName || stateProject === root)) return true;
  if (!root) return false;
  const cwd = (agent.cwd || "").replace(/\\/g, "/").toLowerCase();
  return cwd.endsWith(`/${root}`);
}

function agentButton(agent, role, extraClass = "") {
  const row = document.createElement("button");
  row.type = "button";
  row.className = `side-row ${extraClass}`;
  row.dataset.pane = agent.pane_id;
  row.setAttribute(
    "aria-label",
    [seatLabel(agent), role, SEAT_STATUS[agent.status] || agent.status].filter(Boolean).join(", "),
  );
  const dot = document.createElement("span");
  dot.className = `status-dot status-${agent.status}`;
  dot.setAttribute("aria-hidden", "true");
  const name = document.createElement("span");
  name.className = "side-name";
  name.textContent = seatLabel(agent);
  row.append(dot, name);
  if (role) {
    const roleLabel = document.createElement("span");
    roleLabel.className = "side-role";
    roleLabel.textContent = role;
    row.append(roleLabel);
  }
  return row;
}

export function renderSide(chat, agents) {
  const bootstrap = chat.home ? chat.home.bootstrap : null;
  const alfred = bootstrap ? bootstrap.alfred : null;
  $("side-alfred-name").textContent = alfred ? alfred.name : "Alfred";
  paintAvatar($("side-alfred-avatar"), {
    name: alfred ? alfred.name : "Alfred",
    characterId: "alfred",
    status: alfred && alfred.status,
    px: 30,
  });
  $("side-alfred-status").textContent = alfred
    ? SEAT_STATUS[alfred.status] || alfred.status || t("chat.seat.ready")
    : t("chat.side.offline");
  setNavBadge(bootstrap && bootstrap.chat ? bootstrap.chat.unread : 0);

  const live = agents || [];
  const projects = $("side-projects");
  projects.replaceChildren();
  const projectRows = bootstrap ? bootstrap.projects || [] : [];

  if (!projectRows.length) {
    const empty = document.createElement("p");
    empty.className = "side-empty";
    empty.textContent = t("chat.side.no_projects");
    projects.append(empty);
  } else {
    for (const project of projectRows) {
      const card = document.createElement("div");
      card.className = "project-card";
      // The folder and the state sit in the tooltip; the card shows the name and one dot.
      card.title = [project.root_label, project.status === "active" ? t("chat.project.active") : project.status]
        .filter(Boolean)
        .join(" · ");
      const head = document.createElement("div");
      head.className = "project-head";
      const name = document.createElement("span");
      name.className = "project-name";
      name.textContent = project.name;
      // The grey dot said nothing the lead's own status dot does not: only the name stays.
      head.append(name);
      card.append(head);

      const team = document.createElement("div");
      team.className = "project-team";
      const lead = project.lead;
      if (lead) {
        const leadAgent = lead.pane_id ? live.find((agent) => agent.pane_id === lead.pane_id) : null;
        // The lead's name is on the tooltip only: the card already names the project.
        if (leadAgent) {
          const button = agentButton(
            { ...leadAgent, title: t("chat.side.lead_title", { name: lead.name }), agent: "lead" },
            null,
            "project-lead",
          );
          button.title = t("chat.side.lead_title", { name: lead.name });
          button.querySelector(".side-name").classList.add("sr-only");
          team.append(button);
        } else {
          const row = document.createElement("div");
          row.className = "side-row project-lead is-offline";
          const dot = document.createElement("span");
          dot.className = `status-dot status-${lead.status || "unknown"}`;
          dot.setAttribute("aria-hidden", "true");
          const label = document.createElement("span");
          label.className = "side-name sr-only";
          label.textContent = lead.name || t("chat.side.lead_fallback");
          row.title = t("chat.side.lead_offline", { name: lead.name || t("chat.side.lead_fallback") });
          row.append(dot, label);
          team.append(row);
        }
      }
      const specialists = lead
        ? live.filter(
            (agent) =>
              agent.pane_id !== lead.pane_id && matchesProject(agent, project),
          )
        : live.filter((agent) => matchesProject(agent, project));
      for (const agent of specialists) {
        team.append(agentButton(agent, seatRole(agent)));
      }
      if (!lead && !specialists.length) {
        const empty = document.createElement("p");
        empty.className = "side-empty-project";
        empty.textContent = t("chat.side.no_team_panel");
        team.append(empty);
      }
      card.append(team);
      projects.append(card);
    }
  }

  const team = $("side-team");
  const section = $("side-team-section");
  team.replaceChildren();
  section.classList.toggle("hidden", projectRows.length > 0);
  if (projectRows.length) return;
  if (!live.length) {
    const empty = document.createElement("p");
    empty.className = "side-empty";
    empty.textContent = alfred
      ? t("chat.side.alone", { name: alfred.name })
      : t("chat.side.team_here");
    team.append(empty);
    return;
  }
  for (const agent of live) {
    team.append(agentButton(agent, agent.project || t("chat.seat.pane")));
  }
}
