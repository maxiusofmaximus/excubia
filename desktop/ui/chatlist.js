import { isBlankChat, rowBadge } from "./chat-row-model.js";
import { $ } from "./core.js";
import { setNavBadge } from "./nav.js";
import { emptyState } from "./empty-state.js";
import { paintAvatar } from "./avatar.js";
import { EMPTY_TEXT, FILTERS } from "./chat-filter-model.js";
import { progressText } from "./chat-progress-model.js";
import { prune } from "./chat-select-model.js";

const dayNames = ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"];
const monthNames = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];

export function formatTime(iso) {
  const date = iso ? new Date(iso) : null;
  if (!date || Number.isNaN(date.valueOf())) return "";
  return date.toLocaleTimeString("es-ES", { hour: "2-digit", minute: "2-digit" });
}

export function formatWhen(iso) {
  const date = iso ? new Date(iso) : null;
  if (!date || Number.isNaN(date.valueOf())) return "";
  const now = new Date();
  const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate()).valueOf();
  const value = date.valueOf();
  const dayMs = 86_400_000;
  if (value >= midnight) return formatTime(iso);
  if (value >= midnight - dayMs) return "ayer";
  if (value >= midnight - 6 * dayMs) return dayNames[date.getDay()];
  if (date.getFullYear() === now.getFullYear()) {
    return `${date.getDate()} ${monthNames[date.getMonth()]}`;
  }
  return `${date.getDate()} ${monthNames[date.getMonth()]} ${date.getFullYear()}`;
}

// `view` is { selectedId, progress }: the open conversation and the steps of the running ones.
function row(conversation, view) {
  const element = document.createElement("div");
  element.className = "chat-row";
  element.dataset.id = conversation.id;
  if (conversation.id === view.selectedId) element.classList.add("is-active");
  if (conversation.unread > 0) element.classList.add("is-unread");

  // The option of the history listbox; paintSelection sets aria-selected on each render.
  const main = document.createElement("button");
  main.type = "button";
  main.className = "chat-row-main";
  main.setAttribute("role", "option");
  main.setAttribute("aria-selected", "false");
  main.dataset.select = conversation.id;
  main.setAttribute(
    "aria-label",
    `${conversation.title}${conversation.unread ? `, ${conversation.unread} sin leer` : ""}`,
  );

  const title = document.createElement("span");
  title.className = "chat-row-title";
  title.textContent = `${conversation.pinned ? "★ " : ""}${conversation.title}`;
  title.title = conversation.title;

  const meta = document.createElement("span");
  meta.className = "chat-row-meta";
  // Every row is with Alfred, so the meta line says only the time and the state.
  const when = formatWhen(conversation.last_message_at);
  const parts = [];
  if (conversation.archived) parts.push("archivado");
  if (when) parts.push(when);
  if (conversation.unread) parts.push(`${conversation.unread} sin leer`);
  const step = progressText(view.progress, conversation.id);
  meta.textContent = step || parts.join(" · ") || "Sin mensajes";
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
  more.setAttribute("aria-label", `Acciones de ${conversation.title}`);
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
      action.textContent = options.actionLabel || "Ver";
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
    ? `${progress.archived ? "Archivando" : "Desarchivando"}… ${progress.done} de ${progress.total}`
    : `${count} seleccionados`;
}

// Runs after each history render: drops the rows that left, marks the picked ones.
export function paintSelection(chat) {
  const rows = [...$("chat-groups").querySelectorAll(".chat-row[data-id]")];
  prune(chat.selection, rows.map((element) => element.dataset.id));
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
      empty.textContent = `Sin resultados para «${chat.query}».`;
      groups.append(empty);
    } else {
      groups.append(
        group(`Resultados · ${chat.searchResults.length}`, chat.searchResults, view),
      );
    }
    return;
  }

  if (chat.showArchived) {
    if (!archived.length) {
      const empty = document.createElement("p");
      empty.className = "history-empty";
      empty.textContent = "No hay conversaciones archivadas.";
      groups.append(empty);
    } else {
      groups.append(group("Archivados", archived, view));
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
      emptyState({ icon: "chat", text: "Aún no hay conversaciones. Empieza con «Nuevo chat».", action: "Nuevo chat" }, onNew),
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
  if (pinned.length) groups.append(group("Fijados", pinned, view));
  if (recent.length) groups.append(group(labelled ? "Recientes" : null, recent, view));

  if (sections.length) {
    groups.append(caption("Secciones"));
    for (const section of sections) {
      const rows = inSections.filter((conversation) => conversation.section_id === section.id);
      groups.append(group(section.name, rows, view));
    }
  }
}

const SEAT_STATUS = {
  draft: "disponible",
  idle: "disponible",
  ready: "disponible",
  working: "trabajando",
  running: "trabajando",
  blocked: "bloqueado",
  done: "terminó",
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
  return agent.agent || "Especialista";
}

function seatRole(agent) {
  const split = splitSeat((agent.title || "").trim());
  if (split) return split[1];
  return agent.agent || "especialista";
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
    ? SEAT_STATUS[alfred.status] || alfred.status || "disponible"
    : "Sin conexión";
  setNavBadge(bootstrap && bootstrap.chat ? bootstrap.chat.unread : 0);

  const live = agents || [];
  const projects = $("side-projects");
  projects.replaceChildren();
  const projectRows = bootstrap ? bootstrap.projects || [] : [];

  if (!projectRows.length) {
    const empty = document.createElement("p");
    empty.className = "side-empty";
    empty.textContent = "Sin proyectos todavía.";
    projects.append(empty);
  } else {
    for (const project of projectRows) {
      const card = document.createElement("div");
      card.className = "project-card";
      // The folder and the state sit in the tooltip; the card shows the name and one dot.
      card.title = [project.root_label, project.status === "active" ? "activo" : project.status]
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
            { ...leadAgent, title: `${lead.name} — responsable del proyecto`, agent: "lead" },
            null,
            "project-lead",
          );
          button.title = `${lead.name} — responsable del proyecto`;
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
          label.textContent = lead.name || "Responsable";
          row.title = `${lead.name || "Responsable"} · responsable del proyecto, sin panel abierto`;
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
        empty.textContent = "Sin equipo con panel.";
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
      ? `${alfred.name} está solo: aún no hay paneles de agentes.`
      : "El equipo aparecerá aquí.";
    team.append(empty);
    return;
  }
  for (const agent of live) {
    team.append(agentButton(agent, agent.project || "panel"));
  }
}
